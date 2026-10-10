use super::gateway::tokens_expiring;
use super::{FakePort, TranscriptionBehavior, request};
use crate::application::error::AppError;
use crate::application::ports::{
    ClockPort, DriveAuthPort, DriveAuthorize, DriveCallback, DriveFilesPort, DriveSessionStore,
    LocalUpload, PickedFile, RemoteFolder,
};
use crate::application::use_cases::Application;
use crate::domain::artifact::ArtifactKind;
use crate::domain::drive::{
    DriveAccount, DriveConnection, DriveFileId, DriveStatus, DriveSyncState, DriveTokenFailure,
    DriveTokens, MeetingKey, RemoteFile, parse_picked_ids,
};
use crate::domain::gateway::GatewayRefreshFailure;
use crate::domain::job::{SpeakerHint, TranscriptImportRequest, TranscriptionRequest};
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{Notify, oneshot};
use uuid::Uuid;

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, AppError> {
    mutex
        .lock()
        .map_err(|_| AppError::new("TEST_ERROR", "drive fake lock poisoned"))
}

#[derive(Default)]
struct Stored {
    connection: Option<DriveConnection>,
    tokens: Option<DriveTokens>,
    dismissed: bool,
}

#[derive(Default)]
struct Calls {
    consents: Vec<(bool, Option<String>)>,
    exchange_bearers: Vec<String>,
    refresh_bearers: Vec<String>,
    revoked: Vec<String>,
    created: Vec<ArtifactKind>,
    updated: Vec<String>,
}

/// How the next browser consent ends.
enum Consent {
    /// Come back at once with these picked ids.
    Approve(&'static str),
    /// Wait until released (then approve with the default pick) or cancelled.
    Wait,
}

#[derive(Clone)]
struct RemoteEntry {
    id: String,
    checksum: Option<String>,
    synced: Option<String>,
}

#[derive(Default)]
struct RemoteDrive {
    root: Option<String>,
    meetings: Vec<(String, String)>,
    files: HashMap<(String, ArtifactKind), RemoteEntry>,
    next: usize,
}

impl RemoteDrive {
    fn new_id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}-{}", self.next)
    }
}

/// The Drive side of `FakePort`: an in-memory store, a scripted consent and
/// gateway, and an in-memory Drive that answers like the real one.
pub(super) struct DriveFake {
    stored: Mutex<Stored>,
    calls: Mutex<Calls>,
    consents: Mutex<VecDeque<Consent>>,
    consent_started: Notify,
    consent_release: Notify,
    exchanges: Mutex<VecDeque<Result<DriveTokens, DriveTokenFailure>>>,
    refreshes: Mutex<VecDeque<Result<DriveTokens, DriveTokenFailure>>>,
    account: Mutex<DriveAccount>,
    remote: Mutex<RemoteDrive>,
    local: Mutex<HashMap<PathBuf, String>>,
    picked: Mutex<PickedFile>,
    reject_files: AtomicBool,
    /// Uploads of this kind fail with this code.
    failing_kind: Mutex<Option<(ArtifactKind, &'static str)>>,
    block_download: AtomicBool,
    download_started: Notify,
}

impl Default for DriveFake {
    fn default() -> Self {
        Self {
            stored: Mutex::new(Stored::default()),
            calls: Mutex::new(Calls::default()),
            consents: Mutex::new(VecDeque::new()),
            consent_started: Notify::new(),
            consent_release: Notify::new(),
            exchanges: Mutex::new(VecDeque::new()),
            refreshes: Mutex::new(VecDeque::new()),
            account: Mutex::new(DriveAccount {
                email: "user@example.com".to_owned(),
                permission_id: "perm-1".to_owned(),
            }),
            remote: Mutex::new(RemoteDrive::default()),
            local: Mutex::new(HashMap::new()),
            picked: Mutex::new(PickedFile {
                name: "주간 회의.m4a".to_owned(),
                mime_type: "audio/x-m4a".to_owned(),
                can_download: true,
                modified_time: Some("2026-10-01T09:30:00Z".to_owned()),
            }),
            reject_files: AtomicBool::new(false),
            failing_kind: Mutex::new(None),
            block_download: AtomicBool::new(false),
            download_started: Notify::new(),
        }
    }
}

impl DriveFake {
    fn stored(&self) -> Result<MutexGuard<'_, Stored>, AppError> {
        lock(&self.stored)
    }

    fn calls(&self) -> Result<MutexGuard<'_, Calls>, AppError> {
        lock(&self.calls)
    }

    fn remote(&self) -> Result<MutexGuard<'_, RemoteDrive>, AppError> {
        lock(&self.remote)
    }

    fn push_consent(&self, consent: Consent) -> Result<(), AppError> {
        lock(&self.consents)?.push_back(consent);
        Ok(())
    }

    fn script_refresh(
        &self,
        result: Result<DriveTokens, DriveTokenFailure>,
    ) -> Result<(), AppError> {
        lock(&self.refreshes)?.push_back(result);
        Ok(())
    }

    fn rejecting(&self) -> Result<(), AppError> {
        if self.reject_files.load(Ordering::Relaxed) {
            return Err(AppError::new("DRIVE_TOKEN_REJECTED", "rejected by test"));
        }
        Ok(())
    }

    fn failing(&self, kind: ArtifactKind) -> Result<(), AppError> {
        match *lock(&self.failing_kind)? {
            Some((failing, code)) if failing == kind => Err(AppError::new(code, "저장 공간 부족")),
            _ => Ok(()),
        }
    }

    /// Someone edits the Drive copy of `kind` outside Galpi.
    fn edit_remote(&self, kind: ArtifactKind) -> Result<(), AppError> {
        for ((_, entry_kind), entry) in &mut self.remote()?.files {
            if *entry_kind == kind {
                entry.checksum = Some("edited-elsewhere".to_owned());
            }
        }
        Ok(())
    }

    fn remote_id(&self, kind: ArtifactKind) -> Result<Option<String>, AppError> {
        Ok(self
            .remote()?
            .files
            .iter()
            .find(|((_, entry_kind), _)| *entry_kind == kind)
            .map(|(_, entry)| entry.id.clone()))
    }

    fn remote_checksum(&self, kind: ArtifactKind) -> Result<Option<String>, AppError> {
        Ok(self
            .remote()?
            .files
            .iter()
            .find(|((_, entry_kind), _)| *entry_kind == kind)
            .and_then(|(_, entry)| entry.checksum.clone()))
    }
}

fn drive_tokens(port: &FakePort, name: &str, seconds_left: u64) -> DriveTokens {
    DriveTokens {
        access_token: format!("galpi-test-drive-access-{name}"),
        refresh_token: format!("galpi-test-drive-refresh-{name}"),
        expires_at: port.now_unix() + seconds_left,
    }
}

fn callback(picked: &str) -> Result<DriveCallback, AppError> {
    Ok(DriveCallback {
        code: "galpi-test-code".to_owned(),
        verifier: "galpi-test-verifier".to_owned(),
        redirect_uri: "http://127.0.0.1:1455/auth/callback".to_owned(),
        picked: parse_picked_ids(picked).map_err(|_| AppError::new("TEST_ERROR", "bad pick"))?,
    })
}

#[async_trait]
impl DriveAuthPort for FakePort {
    async fn authorize(
        &self,
        request: &DriveAuthorize,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<DriveCallback, AppError> {
        self.drive
            .calls()?
            .consents
            .push((request.picker, request.login_hint.clone()));
        let default_pick = if request.picker { "file-1" } else { "" };
        let consent = lock(&self.drive.consents)?.pop_front();
        match consent {
            Some(Consent::Approve(picked)) => callback(picked),
            None => callback(default_pick),
            Some(Consent::Wait) => {
                self.drive.consent_started.notify_one();
                tokio::select! {
                    _ = &mut *cancel => Err(AppError::new("CANCELLED", "사용자가 작업을 취소했습니다.")),
                    () = self.drive.consent_release.notified() => callback(default_pick),
                }
            }
        }
    }

    async fn exchange(
        &self,
        bearer: &str,
        _callback: &DriveCallback,
    ) -> Result<DriveTokens, DriveTokenFailure> {
        if let Ok(mut calls) = self.drive.calls() {
            calls.exchange_bearers.push(bearer.to_owned());
        }
        self.drive
            .exchanges
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front())
            .unwrap_or_else(|| Ok(drive_tokens(self, "exchanged", 3_600)))
    }

    async fn refresh(
        &self,
        bearer: &str,
        _refresh_token: &str,
    ) -> Result<DriveTokens, DriveTokenFailure> {
        if let Ok(mut calls) = self.drive.calls() {
            calls.refresh_bearers.push(bearer.to_owned());
        }
        self.drive
            .refreshes
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front())
            .unwrap_or(Err(DriveTokenFailure::Unavailable))
    }

    async fn revoke(&self, refresh_token: &str) {
        if let Ok(mut calls) = self.drive.calls() {
            calls.revoked.push(refresh_token.to_owned());
        }
    }
}

#[async_trait]
impl DriveFilesPort for FakePort {
    async fn whoami(&self, _access_token: &str) -> Result<DriveAccount, AppError> {
        self.drive.rejecting()?;
        Ok(lock(&self.drive.account)?.clone())
    }

    async fn ensure_root_folder(
        &self,
        _access_token: &str,
        _known: Option<&str>,
    ) -> Result<String, AppError> {
        self.drive.rejecting()?;
        // One marked `Galpi` folder at most, like the real lookup.
        let mut remote = self.drive.remote()?;
        if let Some(root) = remote.root.clone() {
            return Ok(root);
        }
        let root = remote.new_id("root");
        remote.root = Some(root.clone());
        Ok(root)
    }

    async fn ensure_meeting_folder(
        &self,
        _access_token: &str,
        _root: &str,
        key: &MeetingKey,
        _name: &str,
    ) -> Result<RemoteFolder, AppError> {
        self.drive.rejecting()?;
        let mut remote = self.drive.remote()?;
        let existing = remote
            .meetings
            .iter()
            .find(|(meeting, _)| meeting == key.as_str())
            .map(|(_, id)| id.clone());
        let id = if let Some(id) = existing {
            id
        } else {
            let id = remote.new_id("meeting");
            remote.meetings.push((key.as_str().to_owned(), id.clone()));
            id
        };
        Ok(RemoteFolder {
            link: Some(format!("https://drive.google.com/drive/folders/{id}")),
            id,
        })
    }

    async fn find_artifact(
        &self,
        _access_token: &str,
        folder: &str,
        kind: ArtifactKind,
    ) -> Result<Option<RemoteFile>, AppError> {
        self.drive.rejecting()?;
        Ok(self
            .drive
            .remote()?
            .files
            .get(&(folder.to_owned(), kind))
            .map(|entry| RemoteFile {
                id: entry.id.clone(),
                sha256_checksum: entry.checksum.clone(),
                synced_sha256: entry.synced.clone(),
                web_view_link: Some(format!("https://drive.google.com/file/d/{}", entry.id)),
            }))
    }

    async fn read_local(&self, path: &Path) -> Result<LocalUpload, AppError> {
        let content = lock(&self.drive.local)?
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        Ok(LocalUpload {
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            // The fake's "checksum" is the content itself: equal content, equal sum.
            sha256: content.clone(),
            content: content.into_bytes(),
        })
    }

    async fn create_file(
        &self,
        _access_token: &str,
        folder: &str,
        _key: &MeetingKey,
        kind: ArtifactKind,
        upload: &LocalUpload,
    ) -> Result<RemoteFile, AppError> {
        self.drive.rejecting()?;
        self.drive.failing(kind)?;
        let mut remote = self.drive.remote()?;
        let id = remote.new_id("file");
        remote.files.insert(
            (folder.to_owned(), kind),
            RemoteEntry {
                id: id.clone(),
                checksum: Some(upload.sha256.clone()),
                synced: Some(upload.sha256.clone()),
            },
        );
        self.drive.calls()?.created.push(kind);
        Ok(RemoteFile {
            web_view_link: Some(format!("https://drive.google.com/file/d/{id}")),
            id,
            sha256_checksum: Some(upload.sha256.clone()),
            synced_sha256: Some(upload.sha256.clone()),
        })
    }

    async fn update_file(
        &self,
        _access_token: &str,
        file_id: &str,
        _key: &MeetingKey,
        kind: ArtifactKind,
        upload: &LocalUpload,
    ) -> Result<RemoteFile, AppError> {
        self.drive.rejecting()?;
        self.drive.failing(kind)?;
        for entry in self.drive.remote()?.files.values_mut() {
            if entry.id == file_id {
                entry.checksum = Some(upload.sha256.clone());
                entry.synced = Some(upload.sha256.clone());
            }
        }
        self.drive.calls()?.updated.push(file_id.to_owned());
        Ok(RemoteFile {
            id: file_id.to_owned(),
            sha256_checksum: Some(upload.sha256.clone()),
            synced_sha256: Some(upload.sha256.clone()),
            web_view_link: None,
        })
    }

    async fn file_metadata(
        &self,
        _access_token: &str,
        _id: &DriveFileId,
    ) -> Result<PickedFile, AppError> {
        self.drive.rejecting()?;
        Ok(lock(&self.drive.picked)?.clone())
    }

    async fn download(
        &self,
        _access_token: &str,
        id: &DriveFileId,
        file: &PickedFile,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<PathBuf, AppError> {
        self.drive.rejecting()?;
        if self.drive.block_download.load(Ordering::Relaxed) {
            self.drive.download_started.notify_one();
            let _cancelled = cancel.await;
            return Err(AppError::new("CANCELLED", "사용자가 작업을 취소했습니다."));
        }
        Ok(PathBuf::from(format!(
            "/tmp/drive-imports/{}/{}",
            id.as_str(),
            file.name
        )))
    }
}

#[async_trait]
impl DriveSessionStore for FakePort {
    async fn load_status(&self) -> Result<DriveStatus, AppError> {
        let stored = self.drive.stored()?;
        Ok(match &stored.connection {
            Some(connection) => DriveStatus::Connected {
                email: connection.email.clone(),
            },
            None => DriveStatus::Disconnected {
                onboarding_dismissed: stored.dismissed,
            },
        })
    }

    async fn load_connection(&self) -> Result<Option<DriveConnection>, AppError> {
        Ok(self.drive.stored()?.connection.clone())
    }

    async fn load_tokens(&self) -> Result<Option<DriveTokens>, AppError> {
        Ok(self.drive.stored()?.tokens.clone())
    }

    async fn save_connection(
        &self,
        connection: DriveConnection,
        tokens: DriveTokens,
    ) -> Result<(), AppError> {
        let mut stored = self.drive.stored()?;
        stored.connection = Some(connection);
        stored.tokens = Some(tokens);
        Ok(())
    }

    async fn replace_tokens(&self, tokens: DriveTokens) -> Result<(), AppError> {
        self.drive.stored()?.tokens = Some(tokens);
        Ok(())
    }

    async fn save_folder_id(&self, folder_id: String) -> Result<(), AppError> {
        if let Some(connection) = &mut self.drive.stored()?.connection {
            connection.folder_id = folder_id;
        }
        Ok(())
    }

    async fn dismiss_onboarding(&self) -> Result<(), AppError> {
        self.drive.stored()?.dismissed = true;
        Ok(())
    }

    async fn clear(&self) -> Result<(), AppError> {
        let mut stored = self.drive.stored()?;
        stored.connection = None;
        stored.tokens = None;
        Ok(())
    }
}

/// A signed-in app whose gateway session is good for an hour.
fn signed_in() -> Result<(Arc<FakePort>, Application), AppError> {
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    port.gateway
        .seed(tokens_expiring(&port, "session", 3_600), None)?;
    let app = port.application();
    Ok((port, app))
}

/// A signed-in app with Drive connected to `root-saved`, which exists on Drive.
fn connected() -> Result<(Arc<FakePort>, Application), AppError> {
    let (port, app) = signed_in()?;
    {
        let mut stored = port.drive.stored()?;
        stored.connection = Some(DriveConnection {
            email: "user@example.com".to_owned(),
            permission_id: "perm-1".to_owned(),
            folder_id: "root-saved".to_owned(),
        });
        stored.tokens = Some(drive_tokens(&port, "stored", 3_600));
    }
    port.drive.remote()?.root = Some("root-saved".to_owned());
    Ok((port, app))
}

fn failure<T>(result: Result<T, AppError>) -> Result<AppError, AppError> {
    match result {
        Ok(_) => Err(AppError::new("TEST_ERROR", "expected a failure")),
        Err(error) => Ok(error),
    }
}

fn joined<T>(
    result: Result<Result<T, AppError>, tokio::task::JoinError>,
) -> Result<Result<T, AppError>, AppError> {
    result.map_err(|error| AppError::new("TEST_JOIN", error.to_string()))
}

fn states(result: &crate::domain::drive::DriveSyncResult) -> Vec<(ArtifactKind, DriveSyncState)> {
    result
        .files
        .iter()
        .map(|file| (file.kind, file.state.clone()))
        .collect()
}

fn stored_tokens(port: &FakePort) -> Result<Option<DriveTokens>, AppError> {
    Ok(port.drive.stored()?.tokens.clone())
}

#[tokio::test]
async fn drive_features_require_sign_in() -> Result<(), AppError> {
    // Given
    let port = Arc::new(FakePort::new(TranscriptionBehavior::Success));
    let app = port.signed_out_application();

    // When
    let codes = [
        failure(app.connect_drive().await)?.code,
        failure(app.import_from_drive(Uuid::now_v7()).await)?.code,
        failure(app.sync_meeting_to_drive(Uuid::now_v7(), false).await)?.code,
    ];

    // Then: only the feature commands are closed
    assert_eq!(codes, ["AUTH_REQUIRED", "AUTH_REQUIRED", "AUTH_REQUIRED"]);
    assert_eq!(
        app.load_drive_status().await?,
        DriveStatus::Disconnected {
            onboarding_dismissed: false
        }
    );
    assert_eq!(port.drive.calls()?.consents, Vec::new());
    Ok(())
}

#[tokio::test]
async fn connecting_stores_the_account_and_the_galpi_folder() -> Result<(), AppError> {
    // Given
    let (port, app) = signed_in()?;

    // When
    let status = app.connect_drive().await?;

    // Then
    assert_eq!(
        status,
        DriveStatus::Connected {
            email: "user@example.com".to_owned()
        }
    );
    let stored = port.drive.stored()?;
    let connection = stored
        .connection
        .clone()
        .ok_or_else(|| AppError::new("TEST_ERROR", "no connection"))?;
    assert_eq!(connection.permission_id, "perm-1");
    assert_eq!(connection.folder_id, "root-1");
    assert_eq!(
        stored
            .tokens
            .as_ref()
            .map(|tokens| tokens.refresh_token.as_str()),
        Some("galpi-test-drive-refresh-exchanged")
    );
    assert_eq!(
        port.drive.calls()?.exchange_bearers,
        ["galpi-test-access-session"]
    );
    assert_eq!(port.drive.calls()?.consents, [(false, None)]);
    Ok(())
}

#[tokio::test]
async fn one_connect_waits_at_a_time_and_cancelling_allows_another() -> Result<(), AppError> {
    // Given: a connect waiting for the browser
    let (port, app) = signed_in()?;
    port.drive.push_consent(Consent::Wait)?;
    let app = Arc::new(app);
    let first = {
        let app = Arc::clone(&app);
        tokio::spawn(async move { app.connect_drive().await })
    };
    port.drive.consent_started.notified().await;

    // When
    let second = failure(app.connect_drive().await)?;
    app.cancel_drive_operation()?;
    let first = failure(joined(first.await)?)?;

    // Then
    assert_eq!(second.code, "DRIVE_BUSY");
    assert_eq!(first.code, "CANCELLED");
    assert_eq!(port.drive.stored()?.connection, None);
    assert_eq!(port.drive.calls()?.exchange_bearers, Vec::<String>::new());
    assert!(matches!(
        app.connect_drive().await?,
        DriveStatus::Connected { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn the_exchange_uses_a_gateway_bearer_renewed_after_a_long_consent() -> Result<(), AppError> {
    // Given: a gateway session with two minutes left and a slow consent
    let (port, app) = signed_in()?;
    port.gateway
        .seed(tokens_expiring(&port, "short", 120), None)?;
    port.drive.push_consent(Consent::Wait)?;
    let app = Arc::new(app);
    let connecting = {
        let app = Arc::clone(&app);
        tokio::spawn(async move { app.connect_drive().await })
    };
    port.drive.consent_started.notified().await;

    // When: the session is about to expire by the time the browser returns
    port.chatgpt.advance(100);
    port.gateway
        .script_refresh(Ok(tokens_expiring(&port, "renewed", 3_600)))?;
    port.drive.consent_release.notify_one();
    joined(connecting.await)??;

    // Then
    assert_eq!(
        port.drive.calls()?.exchange_bearers,
        ["galpi-test-access-renewed"]
    );
    Ok(())
}

#[tokio::test]
async fn importing_with_another_account_is_refused_and_its_grant_revoked() -> Result<(), AppError> {
    // Given
    let (port, app) = connected()?;
    lock(&port.drive.account)?.permission_id = "perm-2".to_owned();
    let before = stored_tokens(&port)?;

    // When
    let error = failure(app.import_from_drive(Uuid::now_v7()).await)?;

    // Then
    assert_eq!(error.code, "DRIVE_ACCOUNT_MISMATCH");
    assert!(error.message.contains("user@example.com"));
    assert_eq!(stored_tokens(&port)?, before);
    assert_eq!(
        port.drive.calls()?.revoked,
        ["galpi-test-drive-refresh-exchanged"]
    );
    assert_eq!(
        port.drive.calls()?.consents,
        [(true, Some("user@example.com".to_owned()))]
    );
    // And the job slot is free again
    app.transcribe(request(SpeakerHint::Auto)).await?;
    Ok(())
}

#[tokio::test]
async fn importing_with_the_same_account_keeps_the_new_grant() -> Result<(), AppError> {
    // Given
    let (port, app) = connected()?;
    let job_id = Uuid::now_v7();

    // When
    let imported = app.import_from_drive(job_id).await?;

    // Then
    assert_eq!(imported.job_id, job_id);
    assert_eq!(imported.drive_file_id.as_str(), "file-1");
    assert_eq!(imported.name, "주간 회의.m4a");
    assert_eq!(
        imported.input_path,
        "/tmp/drive-imports/file-1/주간 회의.m4a"
    );
    assert_eq!(
        stored_tokens(&port)?.map(|tokens| tokens.refresh_token),
        Some("galpi-test-drive-refresh-exchanged".to_owned())
    );
    assert_eq!(port.drive.calls()?.revoked, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn picking_a_google_document_or_nothing_imports_nothing() -> Result<(), AppError> {
    // Given
    let (port, app) = connected()?;
    lock(&port.drive.picked)?.mime_type = "application/vnd.google-apps.document".to_owned();

    // When
    let document = failure(app.import_from_drive(Uuid::now_v7()).await)?;
    port.drive.push_consent(Consent::Approve(""))?;
    let nothing = failure(app.import_from_drive(Uuid::now_v7()).await)?;
    port.drive.push_consent(Consent::Approve("a,b"))?;
    let two = failure(app.import_from_drive(Uuid::now_v7()).await)?;

    // Then
    assert_eq!(document.code, "DRIVE_UNSUPPORTED_FILE");
    assert_eq!(nothing.code, "CANCELLED");
    assert_eq!(two.code, "DRIVE_PICK_INVALID");
    Ok(())
}

#[tokio::test]
async fn cancelling_the_import_job_stops_the_picker_wait_and_frees_the_slot() -> Result<(), AppError>
{
    // Given: an import waiting for the Picker
    let (port, app) = connected()?;
    port.drive.push_consent(Consent::Wait)?;
    let app = Arc::new(app);
    let job_id = Uuid::now_v7();
    let importing = {
        let app = Arc::clone(&app);
        tokio::spawn(async move { app.import_from_drive(job_id).await })
    };
    port.drive.consent_started.notified().await;

    // When
    app.cancel(job_id)?;
    let error = failure(joined(importing.await)?)?;

    // Then
    assert_eq!(error.code, "CANCELLED");
    app.transcribe(request(SpeakerHint::Auto)).await?;
    Ok(())
}

#[tokio::test]
async fn cancelling_the_import_job_stops_the_download_and_frees_the_slot() -> Result<(), AppError> {
    // Given: an import that is downloading
    let (port, app) = connected()?;
    port.drive.block_download.store(true, Ordering::Relaxed);
    let app = Arc::new(app);
    let job_id = Uuid::now_v7();
    let importing = {
        let app = Arc::clone(&app);
        tokio::spawn(async move { app.import_from_drive(job_id).await })
    };
    port.drive.download_started.notified().await;

    // When
    app.cancel(job_id)?;
    let error = failure(joined(importing.await)?)?;

    // Then
    assert_eq!(error.code, "CANCELLED");
    app.transcribe(request(SpeakerHint::Auto)).await?;
    Ok(())
}

#[tokio::test]
async fn a_transcription_must_match_the_drive_import_it_claims() -> Result<(), AppError> {
    // Given
    let (port, app) = connected()?;
    let imported = app.import_from_drive(Uuid::now_v7()).await?;
    let from_drive = |drive_source_id: &str| TranscriptionRequest {
        job_id: Uuid::now_v7(),
        input_path: imported.input_path.clone(),
        output_root: "/tmp/output".to_owned(),
        speaker_hint: SpeakerHint::Auto,
        drive_source_id: Some(drive_source_id.to_owned()),
    };

    // When
    let mismatch = failure(app.transcribe(from_drive("someone-else")).await)?;
    let other_file = failure(
        app.transcribe(TranscriptionRequest {
            input_path: "/tmp/input.m4a".to_owned(),
            ..from_drive("file-1")
        })
        .await,
    )?;
    let transcribed = app.transcribe(from_drive("file-1")).await?;
    app.sync_meeting_to_drive(transcribed.job_id, false).await?;

    // Then: refused before any work, and the match lands in the Drive file's folder
    assert_eq!(mismatch.code, "DRIVE_SOURCE_MISMATCH");
    assert_eq!(other_file.code, "DRIVE_SOURCE_MISMATCH");
    assert_eq!(port.prepare_calls.load(Ordering::SeqCst), 1);
    let meetings = port.drive.remote()?.meetings.clone();
    assert_eq!(
        meetings
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["drive:file-1"]
    );
    Ok(())
}

#[tokio::test]
async fn retranscribing_a_local_file_syncs_into_the_same_drive_folder() -> Result<(), AppError> {
    // Given: the same audio transcribed twice (the second into `job 2`)
    let (port, app) = connected()?;
    let first = app.transcribe(request(SpeakerHint::Auto)).await?;
    let second = app.transcribe(request(SpeakerHint::Auto)).await?;
    app.sync_meeting_to_drive(first.job_id, false).await?;
    let first_txt = port.drive.remote_id(ArtifactKind::SpeakerText)?;

    // When
    let result = app.sync_meeting_to_drive(second.job_id, false).await?;

    // Then
    assert_eq!(port.drive.remote()?.meetings.len(), 1);
    assert_eq!(port.drive.remote_id(ArtifactKind::SpeakerText)?, first_txt);
    assert!(result.files.iter().all(|file| matches!(
        file.state,
        DriveSyncState::Updated | DriveSyncState::Unchanged
    )));
    assert_eq!(port.drive.calls()?.created.len(), 2);
    Ok(())
}

#[tokio::test]
async fn syncing_uploads_skips_updates_and_holds_a_conflict_until_overwritten()
-> Result<(), AppError> {
    // Given
    let (port, app) = connected()?;
    let transcribed = app.transcribe(request(SpeakerHint::Auto)).await?;
    let job = transcribed.job_id;
    let txt = PathBuf::from(&transcribed.txt);

    // When / Then: the first sync uploads both files
    let first = app.sync_meeting_to_drive(job, false).await?;
    assert_eq!(
        states(&first),
        [
            (ArtifactKind::Srt, DriveSyncState::Uploaded),
            (ArtifactKind::SpeakerText, DriveSyncState::Uploaded),
        ]
    );
    assert!(first.folder_link.is_some());
    let txt_id = port.drive.remote_id(ArtifactKind::SpeakerText)?;

    // Same content again: nothing changes
    let again = app.sync_meeting_to_drive(job, false).await?;
    assert!(
        again
            .files
            .iter()
            .all(|file| file.state == DriveSyncState::Unchanged)
    );

    // The local transcript changes: the same Drive file is updated
    lock(&port.drive.local)?.insert(txt.clone(), "고친 전사".to_owned());
    let changed = app.sync_meeting_to_drive(job, false).await?;
    assert_eq!(
        states(&changed)[1],
        (ArtifactKind::SpeakerText, DriveSyncState::Updated)
    );
    assert_eq!(port.drive.remote_id(ArtifactKind::SpeakerText)?, txt_id);

    // Someone edits the Drive copy, and the local file changes again
    port.drive.edit_remote(ArtifactKind::SpeakerText)?;
    lock(&port.drive.local)?.insert(txt, "다시 고친 전사".to_owned());
    let conflict = app.sync_meeting_to_drive(job, false).await?;
    assert_eq!(
        states(&conflict)[1],
        (ArtifactKind::SpeakerText, DriveSyncState::Conflict)
    );
    assert_eq!(
        port.drive
            .remote_checksum(ArtifactKind::SpeakerText)?
            .as_deref(),
        Some("edited-elsewhere")
    );

    // Overwriting replaces the Drive copy
    let overwritten = app.sync_meeting_to_drive(job, true).await?;
    assert_eq!(
        states(&overwritten)[1],
        (ArtifactKind::SpeakerText, DriveSyncState::Updated)
    );
    assert_eq!(
        port.drive.remote_checksum(ArtifactKind::SpeakerText)?,
        Some("다시 고친 전사".to_owned())
    );
    Ok(())
}

#[tokio::test]
async fn one_failing_file_does_not_stop_the_others() -> Result<(), AppError> {
    // Given
    let (port, app) = connected()?;
    *lock(&port.drive.failing_kind)? = Some((ArtifactKind::Srt, "DRIVE_STORAGE_FULL"));
    let transcribed = app.transcribe(request(SpeakerHint::Auto)).await?;

    // When
    let result = app.sync_meeting_to_drive(transcribed.job_id, false).await?;

    // Then
    assert_eq!(
        states(&result),
        [
            (
                ArtifactKind::Srt,
                DriveSyncState::Failed {
                    message: "저장 공간 부족".to_owned()
                }
            ),
            (ArtifactKind::SpeakerText, DriveSyncState::Uploaded),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn an_unreachable_drive_ends_the_whole_sync() -> Result<(), AppError> {
    // Given: Drive stops answering at the first upload
    let (port, app) = connected()?;
    *lock(&port.drive.failing_kind)? = Some((ArtifactKind::Srt, "DRIVE_UNAVAILABLE"));
    let transcribed = app.transcribe(request(SpeakerHint::Auto)).await?;

    // When
    let error = failure(app.sync_meeting_to_drive(transcribed.job_id, false).await)?;

    // Then: no further upload is attempted
    assert_eq!(error.code, "DRIVE_UNAVAILABLE");
    assert_eq!(port.drive.calls()?.created, Vec::new());
    Ok(())
}

#[tokio::test]
async fn a_gateway_refusing_the_bearer_at_exchange_signs_the_host_out() -> Result<(), AppError> {
    // Given: the gateway answers the Drive code exchange with 401
    let (port, app) = signed_in()?;
    lock(&port.drive.exchanges)?.push_back(Err(DriveTokenFailure::AuthRequired));

    // When
    let error = failure(app.connect_drive().await)?;

    // Then: the host closes the session the window is about to leave
    assert_eq!(error.code, "AUTH_REQUIRED");
    assert_eq!(port.gateway.stored_tokens()?, None);
    assert_eq!(
        failure(app.transcribe(request(SpeakerHint::Auto)).await)?.code,
        "AUTH_REQUIRED"
    );
    Ok(())
}

#[tokio::test]
async fn minutes_sync_under_the_transcription_job_after_refinement() -> Result<(), AppError> {
    // Given: a transcribed and refined meeting
    let (_port, app) = connected()?;
    app.save_assistant_api_key("zai_key".to_owned()).await?;
    let transcribed = app.transcribe(request(SpeakerHint::Auto)).await?;
    app.sync_meeting_to_drive(transcribed.job_id, false).await?;
    app.refine_transcript(Uuid::now_v7(), transcribed.job_id, &[])
        .await?;

    // When
    let result = app.sync_meeting_to_drive(transcribed.job_id, false).await?;

    // Then
    assert_eq!(
        states(&result)[2],
        (ArtifactKind::Minutes, DriveSyncState::Uploaded)
    );
    Ok(())
}

#[tokio::test]
async fn an_imported_transcript_is_a_meeting_too() -> Result<(), AppError> {
    // Given
    let (_port, app) = connected()?;
    let imported = app
        .import_transcript(TranscriptImportRequest {
            job_id: Uuid::now_v7(),
            input_path: "/tmp/notes.txt".to_owned(),
            output_root: "/tmp/output".to_owned(),
        })
        .await?;

    // When
    let result = app.sync_meeting_to_drive(imported.job_id, false).await?;

    // Then
    assert_eq!(
        states(&result),
        [(ArtifactKind::SpeakerText, DriveSyncState::Uploaded)]
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_renewal_disconnects_drive() -> Result<(), AppError> {
    // Given: Drive tokens about to expire, and Google refuses to renew them
    let (port, app) = connected()?;
    port.drive.stored()?.tokens = Some(drive_tokens(&port, "stored", 60));
    port.drive
        .script_refresh(Err(DriveTokenFailure::Rejected))?;
    let transcribed = app.transcribe(request(SpeakerHint::Auto)).await?;

    // When
    let error = failure(app.sync_meeting_to_drive(transcribed.job_id, false).await)?;

    // Then
    assert_eq!(error.code, "DRIVE_RECONNECT_REQUIRED");
    assert_eq!(port.drive.stored()?.connection, None);
    assert_eq!(stored_tokens(&port)?, None);
    assert_eq!(
        port.drive.calls()?.refresh_bearers,
        ["galpi-test-access-session"]
    );
    Ok(())
}

#[tokio::test]
async fn a_drive_401_renews_once_and_never_resends() -> Result<(), AppError> {
    // Given: Drive refuses the stored token, but renewal works
    let (port, app) = connected()?;
    port.drive.reject_files.store(true, Ordering::Relaxed);
    port.drive
        .script_refresh(Ok(drive_tokens(&port, "renewed", 3_600)))?;
    let transcribed = app.transcribe(request(SpeakerHint::Auto)).await?;

    // When
    let renewed = failure(app.sync_meeting_to_drive(transcribed.job_id, false).await)?;

    // Then: the renewal is kept, and the refused request is not retried
    assert_eq!(renewed.code, "DRIVE_UNAVAILABLE");
    assert_eq!(port.drive.calls()?.refresh_bearers.len(), 1);
    assert_eq!(port.drive.calls()?.created, Vec::new());
    assert_eq!(
        stored_tokens(&port)?.map(|tokens| tokens.access_token),
        Some("galpi-test-drive-access-renewed".to_owned())
    );

    // Given: the renewal is refused too
    port.drive
        .script_refresh(Err(DriveTokenFailure::Rejected))?;

    // When
    let refused = failure(app.sync_meeting_to_drive(transcribed.job_id, false).await)?;

    // Then
    assert_eq!(refused.code, "DRIVE_RECONNECT_REQUIRED");
    assert_eq!(port.drive.stored()?.connection, None);
    Ok(())
}

#[tokio::test]
async fn a_deleted_galpi_folder_is_recreated_and_remembered() -> Result<(), AppError> {
    // Given: the remembered folder is gone from Drive
    let (port, app) = connected()?;
    port.drive.remote()?.root = None;
    let transcribed = app.transcribe(request(SpeakerHint::Auto)).await?;

    // When
    app.sync_meeting_to_drive(transcribed.job_id, false).await?;

    // Then
    let root = port.drive.remote()?.root.clone();
    assert!(root.is_some());
    assert_ne!(root.as_deref(), Some("root-saved"));
    assert_eq!(
        port.drive
            .stored()?
            .connection
            .as_ref()
            .map(|connection| connection.folder_id.clone()),
        root
    );
    Ok(())
}

#[tokio::test]
async fn disconnecting_revokes_and_forgets_but_keeps_the_onboarding_choice() -> Result<(), AppError>
{
    // Given
    let (port, app) = connected()?;
    app.dismiss_drive_onboarding().await?;

    // When
    let status = app.disconnect_drive().await?;

    // Then
    assert_eq!(
        status,
        DriveStatus::Disconnected {
            onboarding_dismissed: true
        }
    );
    assert_eq!(
        port.drive.calls()?.revoked,
        ["galpi-test-drive-refresh-stored"]
    );
    assert_eq!(stored_tokens(&port)?, None);
    assert_eq!(port.drive.stored()?.connection, None);
    Ok(())
}

#[tokio::test]
async fn disconnecting_during_a_picker_consent_keeps_the_store_empty() -> Result<(), AppError> {
    // Given: an import waiting for the Picker
    let (port, app) = connected()?;
    port.drive.push_consent(Consent::Wait)?;
    let app = Arc::new(app);
    let importing = {
        let app = Arc::clone(&app);
        tokio::spawn(async move { app.import_from_drive(Uuid::now_v7()).await })
    };
    port.drive.consent_started.notified().await;

    // When: Drive is disconnected, then the Picker comes back
    app.disconnect_drive().await?;
    port.drive.consent_release.notify_one();
    let error = failure(joined(importing.await)?)?;

    // Then: the new grant is given back, nothing comes back to life
    assert_eq!(error.code, "DRIVE_NOT_CONNECTED");
    assert_eq!(stored_tokens(&port)?, None);
    assert_eq!(port.drive.stored()?.connection, None);
    assert_eq!(
        port.drive.calls()?.revoked,
        [
            "galpi-test-drive-refresh-stored",
            "galpi-test-drive-refresh-exchanged"
        ]
    );
    Ok(())
}

#[tokio::test]
async fn reconnecting_another_account_during_a_picker_consent_keeps_its_tokens()
-> Result<(), AppError> {
    // Given: an import for account A waiting for the Picker
    let (port, app) = connected()?;
    port.drive.push_consent(Consent::Wait)?;
    let app = Arc::new(app);
    let importing = {
        let app = Arc::clone(&app);
        tokio::spawn(async move { app.import_from_drive(Uuid::now_v7()).await })
    };
    port.drive.consent_started.notified().await;

    // When: Drive is reconnected to account B, then A's Picker comes back
    {
        let mut stored = port.drive.stored()?;
        if let Some(connection) = stored.connection.as_mut() {
            connection.email = "other@example.com".to_owned();
            connection.permission_id = "perm-2".to_owned();
        }
        stored.tokens = Some(drive_tokens(&port, "other", 3_600));
    }
    port.drive.consent_release.notify_one();
    let error = failure(joined(importing.await)?)?;

    // Then: A's grant is given back and B keeps its own tokens
    assert_eq!(error.code, "DRIVE_NOT_CONNECTED");
    assert_eq!(
        stored_tokens(&port)?,
        Some(drive_tokens(&port, "other", 3_600))
    );
    assert_eq!(
        port.drive.calls()?.revoked,
        ["galpi-test-drive-refresh-exchanged"]
    );
    Ok(())
}

#[tokio::test]
async fn a_gateway_refusal_during_a_drive_call_signs_the_app_out() -> Result<(), AppError> {
    // Given: an expired gateway session the gateway refuses to renew
    let (port, app) = connected()?;
    port.gateway
        .seed(tokens_expiring(&port, "expired", 0), None)?;
    port.gateway
        .script_refresh(Err(GatewayRefreshFailure::Rejected))?;
    port.drive.stored()?.tokens = Some(drive_tokens(&port, "stored", 0));
    let transcribed = app.transcribe(request(SpeakerHint::Auto)).await?;

    // When
    let error = failure(app.sync_meeting_to_drive(transcribed.job_id, false).await)?;

    // Then: Drive stays connected, but the app is closed until sign-in
    assert_eq!(error.code, "AUTH_REQUIRED");
    assert_eq!(port.gateway.stored_tokens()?, None);
    assert!(port.drive.stored()?.connection.is_some());
    assert_eq!(
        failure(app.transcribe(request(SpeakerHint::Auto)).await)?.code,
        "AUTH_REQUIRED"
    );
    Ok(())
}

#[tokio::test]
async fn syncing_without_a_connection_is_refused() -> Result<(), AppError> {
    // Given
    let (_port, app) = signed_in()?;
    let transcribed = app.transcribe(request(SpeakerHint::Auto)).await?;

    // When
    let error = failure(app.sync_meeting_to_drive(transcribed.job_id, false).await)?;

    // Then
    assert_eq!(error.code, "DRIVE_NOT_CONNECTED");
    Ok(())
}
