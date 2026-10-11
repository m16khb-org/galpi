//! Google Drive use cases: connecting, importing a recording through the
//! desktop Picker, and syncing a meeting's results.
//!
//! The host owns the Drive tokens. Exchanges and renewals go through the
//! auth-gateway, which needs a fresh gateway bearer each time, so the bearer
//! is fetched right before every gateway call rather than once per use case:
//! a consent page can stay open for minutes.
//!
//! Every connection and token write happens under `store_lock` and, apart
//! from connecting, only while a connection is still on file. A disconnect
//! takes the same lock and clears the store, so nothing written by a sync,
//! Picker grant, or token renewal that was in flight can bring the connection
//! back. Dismissing the onboarding card touches neither and takes no lock.

use crate::application::error::AppError;
use crate::application::gateway::{AppAccessGate, auth_required};
use crate::application::ports::{
    ClockPort, DriveAuthPort, DriveAuthorize, DriveCallback, DriveFilesPort, DriveSessionStore,
    LargeUpload,
};
use crate::domain::artifact::{ArtifactKind, Artifacts};
use crate::domain::drive::{
    DRIVE_ACCESS_REFRESH_MARGIN_SECONDS, DriveConnection, DriveFileId, DriveMeeting,
    DriveRecordingState, DriveRecordingUpload, DriveStatus, DriveSyncFile, DriveSyncResult,
    DriveSyncState, DriveTokenFailure, DriveTokens, MeetingKey, SyncDecision, decide,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// The result files a sync uploads, in display order. The checkpoint stays local.
const SYNCED_KINDS: [ArtifactKind; 3] = [
    ArtifactKind::Srt,
    ArtifactKind::SpeakerText,
    ArtifactKind::Minutes,
];

fn not_connected() -> AppError {
    AppError::new("DRIVE_NOT_CONNECTED", "먼저 Google Drive를 연결해 주세요.")
}

fn reconnect_required() -> AppError {
    AppError::new(
        "DRIVE_RECONNECT_REQUIRED",
        "Drive 연결이 만료되었습니다. 다시 연결해 주세요.",
    )
}

fn unavailable() -> AppError {
    AppError::new(
        "DRIVE_UNAVAILABLE",
        "Google Drive 또는 로그인 서버에 연결하지 못했습니다. 잠시 뒤 다시 시도해 주세요.",
    )
}

fn cancelled() -> AppError {
    AppError::new("CANCELLED", "사용자가 작업을 취소했습니다.")
}

fn state_error() -> AppError {
    AppError::new("STATE_ERROR", "Drive 작업 상태 잠금이 손상되었습니다.")
}

fn token_rejected(error: &AppError) -> bool {
    error.code == "DRIVE_TOKEN_REJECTED"
}

/// What a failed gateway token call means to the caller. `rejected` is the
/// answer to Google refusing the grant. A refused bearer means the Galpi
/// session is gone, so the host closes it too, as the window will.
async fn token_failure(
    access: &AppAccessGate,
    failure: DriveTokenFailure,
    rejected: AppError,
) -> AppError {
    match failure {
        DriveTokenFailure::Rejected => rejected,
        DriveTokenFailure::AuthRequired => {
            access.expire().await;
            auth_required()
        }
        DriveTokenFailure::Unavailable => unavailable(),
    }
}

/// A recording the Picker chose and the download finished.
pub struct ImportedRecording {
    pub path: PathBuf,
    pub id: DriveFileId,
    pub name: String,
}

/// Releases the connect slot however the attempt ends, including a dropped call.
struct OperationSlot<'a> {
    drive: &'a DriveAccounts,
}

impl Drop for OperationSlot<'_> {
    fn drop(&mut self) {
        if let Ok(mut cancel) = self.drive.operation_cancel.lock() {
            *cancel = None;
        }
        self.drive.operation_active.store(false, Ordering::Release);
    }
}

pub struct DriveAccounts {
    auth: Arc<dyn DriveAuthPort>,
    files: Arc<dyn DriveFilesPort>,
    store: Arc<dyn DriveSessionStore>,
    clock: Arc<dyn ClockPort>,
    /// Serializes token renewals and every connection or token write, including the clear.
    store_lock: tokio::sync::Mutex<()>,
    /// The waiting connect consent. The Picker uses a job slot instead, so its
    /// wait is cancelled like any other job.
    operation_active: AtomicBool,
    operation_cancel: Mutex<Option<oneshot::Sender<()>>>,
}

impl DriveAccounts {
    pub fn new(
        auth: Arc<dyn DriveAuthPort>,
        files: Arc<dyn DriveFilesPort>,
        store: Arc<dyn DriveSessionStore>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            auth,
            files,
            store,
            clock,
            store_lock: tokio::sync::Mutex::new(()),
            operation_active: AtomicBool::new(false),
            operation_cancel: Mutex::new(None),
        }
    }

    pub async fn status(&self) -> Result<DriveStatus, AppError> {
        self.store.load_status().await
    }

    pub async fn dismiss_onboarding(&self) -> Result<DriveStatus, AppError> {
        self.store.dismiss_onboarding().await?;
        self.store.load_status().await
    }

    /// Choose whether finished recordings upload on their own. Only a
    /// connected account can have the choice; it ends with the connection.
    pub async fn set_save_recordings(&self, enabled: bool) -> Result<DriveStatus, AppError> {
        let _writing = self.store_lock.lock().await;
        if self.store.load_connection().await?.is_none() {
            return Err(not_connected());
        }
        self.store.save_recordings_preference(enabled).await?;
        self.store.load_status().await
    }

    /// Fail with `DRIVE_NOT_CONNECTED` unless a connection is on file.
    async fn connection(&self) -> Result<DriveConnection, AppError> {
        self.store
            .load_connection()
            .await?
            .ok_or_else(not_connected)
    }

    /// Consent in the browser, exchange through the gateway, then remember
    /// the account and the `Galpi` folder.
    pub async fn connect(&self, access: &AppAccessGate) -> Result<DriveStatus, AppError> {
        let (_slot, mut cancel) = self.claim_operation()?;
        let (_callback, tokens) = self
            .consent(
                access,
                &DriveAuthorize {
                    picker: false,
                    login_hint: None,
                },
                &mut cancel,
            )
            .await?;
        let prepared = self.prepare_connection(&tokens).await;
        let _writing = self.store_lock.lock().await;
        // A cancel or disconnect that arrived after the browser came back
        // wins: the new grant is given back instead of stored.
        if cancel.try_recv().is_ok() {
            self.auth.revoke(&tokens.refresh_token).await;
            return Err(cancelled());
        }
        let connection = match prepared {
            Ok(connection) => connection,
            Err(error) => {
                self.auth.revoke(&tokens.refresh_token).await;
                return Err(error);
            }
        };
        self.store.save_connection(connection, tokens).await?;
        self.store.load_status().await
    }

    /// One browser consent, exchanged through the gateway. The bearer is
    /// fetched only after the browser comes back, so a consent left open for
    /// minutes still exchanges with a live gateway session.
    async fn consent(
        &self,
        access: &AppAccessGate,
        request: &DriveAuthorize,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<(DriveCallback, DriveTokens), AppError> {
        let callback = self.auth.authorize(request, cancel).await?;
        let bearer = access.bearer().await?;
        match self.auth.exchange(&bearer, &callback).await {
            Ok(tokens) => Ok((callback, tokens)),
            Err(failure) => {
                let rejected = AppError::new(
                    "DRIVE_CONNECT_FAILED",
                    "Google이 Drive 권한을 확인하지 못했습니다. 다시 시도해 주세요.",
                );
                Err(token_failure(access, failure, rejected).await)
            }
        }
    }

    async fn prepare_connection(&self, tokens: &DriveTokens) -> Result<DriveConnection, AppError> {
        let account = self.files.whoami(&tokens.access_token).await?;
        let folder_id = self
            .files
            .ensure_root_folder(&tokens.access_token, None)
            .await?;
        Ok(DriveConnection {
            email: account.email,
            permission_id: account.permission_id,
            folder_id,
        })
    }

    /// Ask a waiting connect consent to stop. Harmless when none is waiting.
    pub fn cancel(&self) -> Result<(), AppError> {
        let sender = self
            .operation_cancel
            .lock()
            .map_err(|_| state_error())?
            .take();
        if let Some(sender) = sender {
            // The receiver is gone only if the attempt already finished.
            let _already_finished = sender.send(());
        }
        Ok(())
    }

    /// Revoke at Google when possible, then forget the connection whatever
    /// Google said. Files on Drive stay where they are.
    pub async fn disconnect(&self) -> Result<DriveStatus, AppError> {
        self.cancel()?;
        let _writing = self.store_lock.lock().await;
        // Tokens that cannot be read cannot be revoked either, and
        // disconnecting must still finish.
        if let Some(tokens) = self.store.load_tokens().await.ok().flatten() {
            self.auth.revoke(&tokens.refresh_token).await;
        }
        self.store.clear().await?;
        self.store.load_status().await
    }

    /// A Drive access token with at least the refresh margin left.
    async fn access_token(&self, access: &AppAccessGate) -> Result<String, AppError> {
        let _writing = self.store_lock.lock().await;
        let tokens = self.stored_tokens().await?;
        if !tokens.needs_refresh(self.clock.now_unix(), DRIVE_ACCESS_REFRESH_MARGIN_SECONDS) {
            return Ok(tokens.access_token);
        }
        self.renew_locked(access, &tokens).await
    }

    /// Pass a Drive call's result through, except a 401 (see `after_refused_token`).
    async fn recovered<T>(
        &self,
        access: &AppAccessGate,
        result: Result<T, AppError>,
    ) -> Result<T, AppError> {
        match result {
            Err(error) if token_rejected(&error) => Err(self.after_refused_token(access).await),
            result => result,
        }
    }

    /// Answer a Drive 401: renew once whatever the expiry says, then fail the
    /// operation without resending the refused request.
    async fn after_refused_token(&self, access: &AppAccessGate) -> AppError {
        let _writing = self.store_lock.lock().await;
        let renewed = match self.stored_tokens().await {
            Ok(tokens) => self.renew_locked(access, &tokens).await,
            Err(error) => Err(error),
        };
        renewed.err().unwrap_or_else(unavailable)
    }

    /// Tokens of the connection on file; must be called under `store_lock`.
    async fn stored_tokens(&self) -> Result<DriveTokens, AppError> {
        if self.store.load_connection().await?.is_none() {
            return Err(not_connected());
        }
        self.store.load_tokens().await?.ok_or_else(not_connected)
    }

    /// Renew through the gateway; must be called under `store_lock`.
    async fn renew_locked(
        &self,
        access: &AppAccessGate,
        tokens: &DriveTokens,
    ) -> Result<String, AppError> {
        let bearer = access.bearer().await?;
        match self.auth.refresh(&bearer, &tokens.refresh_token).await {
            Ok(renewed) => {
                let access_token = renewed.access_token.clone();
                self.store.replace_tokens(renewed).await?;
                Ok(access_token)
            }
            Err(DriveTokenFailure::Rejected) => {
                self.store.clear().await?;
                Err(reconnect_required())
            }
            Err(failure) => Err(token_failure(access, failure, reconnect_required()).await),
        }
    }

    /// Let the user pick one recording in the desktop Picker and download it.
    ///
    /// `cancel` is the caller's job slot, so the job's cancel stops both the
    /// consent wait and the download.
    pub async fn import(
        &self,
        access: &AppAccessGate,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<ImportedRecording, AppError> {
        let (id, access_token) = self.pick(access, cancel).await?;
        let file = self
            .recovered(access, self.files.file_metadata(&access_token, &id).await)
            .await?;
        let playable = file.mime_type.starts_with("audio/") || file.mime_type.starts_with("video/");
        if !playable {
            return Err(AppError::new(
                "DRIVE_UNSUPPORTED_FILE",
                "오디오나 영상 파일만 가져올 수 있습니다.",
            ));
        }
        if !file.can_download {
            return Err(AppError::new(
                "DRIVE_DOWNLOAD_FAILED",
                "Drive에서 이 파일을 내려받을 권한이 없습니다.",
            ));
        }
        let path = self
            .recovered(
                access,
                self.files.download(&access_token, &id, &file, cancel).await,
            )
            .await?;
        Ok(ImportedRecording {
            path,
            id,
            name: file.name,
        })
    }

    /// The Picker consent: the chosen file and a token that can read it.
    async fn pick(
        &self,
        access: &AppAccessGate,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<(DriveFileId, String), AppError> {
        let connection = self.connection().await?;
        let (callback, tokens) = self
            .consent(
                access,
                &DriveAuthorize {
                    picker: true,
                    login_hint: Some(connection.email.clone()),
                },
                cancel,
            )
            .await?;
        let account = match self.files.whoami(&tokens.access_token).await {
            Ok(account) => account,
            Err(error) => {
                self.auth.revoke(&tokens.refresh_token).await;
                return Err(error);
            }
        };
        if account.permission_id != connection.permission_id {
            self.auth.revoke(&tokens.refresh_token).await;
            return Err(AppError::new(
                "DRIVE_ACCOUNT_MISMATCH",
                format!(
                    "연결된 Drive 계정({})과 다른 계정에서 파일을 골랐습니다.",
                    connection.email
                ),
            ));
        }
        {
            let _writing = self.store_lock.lock().await;
            let current = self.store.load_connection().await?;
            // A disconnect, or a reconnect to another account, during the
            // Picker wait must not attach these tokens to the new connection.
            if current.is_none_or(|current| current.permission_id != account.permission_id) {
                self.auth.revoke(&tokens.refresh_token).await;
                return Err(not_connected());
            }
            // The newest consent covers the picked file; keep it.
            self.store.replace_tokens(tokens.clone()).await?;
        }
        let mut picked = callback.picked.into_iter();
        match (picked.next(), picked.next()) {
            (Some(id), None) => Ok((id, tokens.access_token)),
            (None, _) => Err(cancelled()),
            (Some(_), Some(_)) => Err(AppError::new(
                "DRIVE_PICK_INVALID",
                "녹음 파일은 한 번에 하나만 가져올 수 있습니다.",
            )),
        }
    }

    /// The `Galpi` folder, remembered again when Drive had to replace it.
    async fn ensure_root(
        &self,
        access: &AppAccessGate,
        connection: &DriveConnection,
        token: &str,
    ) -> Result<String, AppError> {
        let root = self
            .recovered(
                access,
                self.files
                    .ensure_root_folder(token, Some(&connection.folder_id))
                    .await,
            )
            .await?;
        if root != connection.folder_id {
            let _writing = self.store_lock.lock().await;
            if self.store.load_connection().await?.is_some() {
                self.store.save_folder_id(root.clone()).await?;
            }
        }
        Ok(root)
    }

    /// Store a finished recording in `Galpi/<meeting>` with a resumable upload.
    ///
    /// The file is read in chunks, never whole. A recording Drive already
    /// holds with the checksum Galpi recorded is left alone, and so is a
    /// Drive copy that changed elsewhere since.
    pub async fn upload_recording(
        &self,
        access: &AppAccessGate,
        meeting: &DriveMeeting,
        path: &Path,
    ) -> Result<DriveRecordingUpload, AppError> {
        let connection = self.connection().await?;
        let digest = self.files.digest_local(path).await?;
        let token = self.access_token(access).await?;
        let root = self.ensure_root(access, &connection, &token).await?;
        let folder = self
            .recovered(
                access,
                self.files
                    .ensure_meeting_folder(&token, &root, &meeting.key, &meeting.name)
                    .await,
            )
            .await?;
        let remote = self
            .recovered(
                access,
                self.files
                    .find_artifact(&token, &folder.id, ArtifactKind::Recording)
                    .await,
            )
            .await?;
        let create = LargeUpload {
            folder: &folder.id,
            key: &meeting.key,
            kind: ArtifactKind::Recording,
            existing: None,
            path,
            digest: &digest,
        };
        let decision = decide(remote.as_ref(), &digest.sha256, false);
        let (state, web_view_link) = match (decision, remote) {
            // `decide` never asks to update a file that is not there.
            (SyncDecision::Create, _) | (SyncDecision::Update, None) => {
                let sent = self
                    .recovered(access, self.files.upload_large(&token, &create).await)
                    .await?;
                (DriveRecordingState::Uploaded, sent.web_view_link)
            }
            (SyncDecision::Update, Some(remote)) => {
                let replace = LargeUpload {
                    existing: Some(&remote.id),
                    ..create
                };
                let sent = self
                    .recovered(access, self.files.upload_large(&token, &replace).await)
                    .await?;
                (
                    DriveRecordingState::Updated,
                    sent.web_view_link.or(remote.web_view_link),
                )
            }
            (SyncDecision::Unchanged, remote) => (
                DriveRecordingState::Unchanged,
                remote.and_then(|remote| remote.web_view_link),
            ),
            (SyncDecision::Conflict, remote) => (
                DriveRecordingState::Conflict,
                remote.and_then(|remote| remote.web_view_link),
            ),
        };
        Ok(DriveRecordingUpload {
            state,
            folder_link: folder.link,
            web_view_link,
        })
    }

    /// Upload or update this meeting's results in `Galpi/<meeting>`.
    ///
    /// One file's failure is reported in its slot and the others continue; a
    /// refused token, an unreachable Drive, or a lost connection ends the
    /// whole sync.
    pub async fn sync(
        &self,
        access: &AppAccessGate,
        artifacts: &Artifacts,
        meeting: &DriveMeeting,
        overwrite: bool,
    ) -> Result<DriveSyncResult, AppError> {
        let connection = self.connection().await?;
        let token = self.access_token(access).await?;
        let root = self.ensure_root(access, &connection, &token).await?;
        let folder = self
            .recovered(
                access,
                self.files
                    .ensure_meeting_folder(&token, &root, &meeting.key, &meeting.name)
                    .await,
            )
            .await?;
        let mut files = Vec::new();
        for kind in SYNCED_KINDS {
            let Some(path) = artifacts.path_for(kind) else {
                continue;
            };
            let synced = self
                .sync_file(&token, &folder.id, &meeting.key, kind, path, overwrite)
                .await;
            match synced {
                Ok(file) => files.push(file),
                Err(error) if token_rejected(&error) => {
                    return Err(self.after_refused_token(access).await);
                }
                // Drive itself is unreachable: the next file would fail the same way.
                Err(error) if error.code == "DRIVE_UNAVAILABLE" => return Err(error),
                Err(error) => files.push(DriveSyncFile {
                    kind,
                    state: DriveSyncState::Failed {
                        message: error.message,
                    },
                    web_view_link: None,
                }),
            }
        }
        Ok(DriveSyncResult {
            folder_link: folder.link,
            files,
        })
    }

    async fn sync_file(
        &self,
        token: &str,
        folder: &str,
        key: &MeetingKey,
        kind: ArtifactKind,
        path: &Path,
        overwrite: bool,
    ) -> Result<DriveSyncFile, AppError> {
        let upload = self.files.read_local(path).await?;
        let remote = self.files.find_artifact(token, folder, kind).await?;
        let (state, web_view_link) =
            match (decide(remote.as_ref(), &upload.sha256, overwrite), remote) {
                // `decide` never asks to update a file that is not there.
                (SyncDecision::Create, _) | (SyncDecision::Update, None) => {
                    let created = self
                        .files
                        .create_file(token, folder, key, kind, &upload)
                        .await?;
                    (DriveSyncState::Uploaded, created.web_view_link)
                }
                (SyncDecision::Update, Some(remote)) => {
                    let updated = self
                        .files
                        .update_file(token, &remote.id, key, kind, &upload)
                        .await?;
                    (
                        DriveSyncState::Updated,
                        updated.web_view_link.or(remote.web_view_link),
                    )
                }
                (SyncDecision::Unchanged, remote) => (
                    DriveSyncState::Unchanged,
                    remote.and_then(|remote| remote.web_view_link),
                ),
                (SyncDecision::Conflict, remote) => (
                    DriveSyncState::Conflict,
                    remote.and_then(|remote| remote.web_view_link),
                ),
            };
        Ok(DriveSyncFile {
            kind,
            state,
            web_view_link,
        })
    }

    fn claim_operation(&self) -> Result<(OperationSlot<'_>, oneshot::Receiver<()>), AppError> {
        if self.operation_active.swap(true, Ordering::AcqRel) {
            return Err(AppError::new(
                "DRIVE_BUSY",
                "이미 Google Drive 연결이 진행 중입니다.",
            ));
        }
        let slot = OperationSlot { drive: self };
        let (sender, receiver) = oneshot::channel();
        *self.operation_cancel.lock().map_err(|_| state_error())? = Some(sender);
        Ok((slot, receiver))
    }
}
