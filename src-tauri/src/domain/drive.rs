//! Google Drive: the stored connection, file references, the identity of a
//! meeting on Drive, and the stateless decision of what one sync does.
//!
//! `DriveTokens` is secret and never travels in any IPC type: its `Debug`
//! hides every value and only the secret store serializes it. Everything the
//! window sees (`DriveStatus`, `DriveImport`, `DriveSyncResult`) holds no token.

use crate::domain::artifact::{ArtifactKind, is_transcript_name, recording_name};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Debug, Formatter};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

/// Refresh the Drive access token once less than this many seconds remain.
pub const DRIVE_ACCESS_REFRESH_MARGIN_SECONDS: u64 = 5 * 60;

/// The `appProperties` key that carries a meeting key.
pub const MEETING_PROPERTY: &str = "galpiMeeting";

/// Drive limits one `appProperties` entry to 124 bytes of key plus value.
const APP_PROPERTY_BYTES: usize = 124;

/// The Google OAuth tokens for `drive.file`. Serialized only by the secret store.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveTokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds.
    pub expires_at: u64,
}

impl DriveTokens {
    pub fn needs_refresh(&self, now: u64, margin: u64) -> bool {
        now >= self.expires_at.saturating_sub(margin)
    }
}

impl Debug for DriveTokens {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DriveTokens")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Why exchanging or renewing Drive tokens at Google failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveTokenFailure {
    /// Google refused the code or refresh token (`invalid_grant`).
    Rejected,
    /// Google refused the stored client itself (`invalid_client`,
    /// `unauthorized_client`): a wrong ID or secret, or a deleted client.
    ClientRejected,
    /// Anything else: network, Google, or an unexpected answer.
    Unavailable,
}

/// Who the stored Drive grant belongs to and where Galpi keeps its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveConnection {
    pub email: String,
    /// Google's stable id for the account, used to match a later Picker grant.
    pub permission_id: String,
    pub folder_id: String,
}

/// The Google account a token belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveAccount {
    pub email: String,
    pub permission_id: String,
}

/// What the window shows about Drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum DriveStatus {
    #[serde(rename_all = "camelCase")]
    Disconnected { onboarding_dismissed: bool },
    #[serde(rename_all = "camelCase")]
    Connected {
        email: String,
        /// Upload each finished microphone recording right away. Cleared on
        /// disconnect, so a later connection, maybe to another account, never
        /// starts uploading recordings by itself.
        save_recordings: bool,
    },
}

/// A Google OAuth client (desktop app type): its id and secret.
///
/// The person brings their own: `Debug` hides the secret, it has no
/// `Serialize`, and no IPC type carries it.
#[derive(Clone, PartialEq, Eq)]
pub struct DriveClient {
    id: String,
    secret: String,
}

/// Which field of a [`DriveClient`] is malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveClientError {
    Id,
    Secret,
}

const CLIENT_ID_SUFFIX: &str = ".apps.googleusercontent.com";
const CLIENT_SECRET_MAX_CHARS: usize = 256;

impl DriveClient {
    /// Trims both fields. The id must read `<digits>-<a-z0-9>.apps.googleusercontent.com`;
    /// the secret must be non-empty without whitespace. Older clients' secrets
    /// have no `GOCSPX-` prefix, so no prefix is required.
    pub fn parse(id: &str, secret: &str) -> Result<Self, DriveClientError> {
        let id = id.trim();
        let secret = secret.trim();
        let well_formed_id = id.strip_suffix(CLIENT_ID_SUFFIX).is_some_and(|stem| {
            stem.split_once('-').is_some_and(|(project, key)| {
                !project.is_empty()
                    && project.bytes().all(|byte| byte.is_ascii_digit())
                    && !key.is_empty()
                    && key
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            })
        });
        if !well_formed_id {
            return Err(DriveClientError::Id);
        }
        if secret.is_empty()
            || secret.chars().any(char::is_whitespace)
            || secret.chars().count() > CLIENT_SECRET_MAX_CHARS
        {
            return Err(DriveClientError::Secret);
        }
        Ok(Self {
            id: id.to_owned(),
            secret: secret.to_owned(),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn secret(&self) -> &str {
        &self.secret
    }
}

impl Debug for DriveClient {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DriveClient")
            .field("id", &self.id)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// Which client the window sees Drive using. Never holds the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveClientStatus {
    /// `true` once the person saved a client; Drive cannot connect without one.
    pub configured: bool,
    pub client_id: Option<String>,
}

impl DriveClientStatus {
    pub fn of(client: Option<&DriveClient>) -> Self {
        Self {
            configured: client.is_some(),
            client_id: client.map(|client| client.id().to_owned()),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DriveIdError {
    #[error("Drive file id must be 1-200 characters of A-Z, a-z, 0-9, '-' or '_'")]
    Invalid,
}

/// A Drive file id. Only URL- and query-safe characters are accepted, so an id
/// can be placed in a request path or a `q` expression without escaping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct DriveFileId(String);

impl DriveFileId {
    pub fn parse(value: &str) -> Result<Self, DriveIdError> {
        let valid = (1..=200).contains(&value.len())
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(DriveIdError::Invalid)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The Picker callback's comma-separated `picked_file_ids`; empty means none.
pub fn parse_picked_ids(value: &str) -> Result<Vec<DriveFileId>, DriveIdError> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|id| DriveFileId::parse(id.trim()))
        .collect()
}

/// FNV-1a 64 as 16 hex digits. Unlike std's hasher it never changes between
/// builds or platforms, so a key written to Drive today still matches later.
fn stable_hash(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// Which Drive meeting folder a local meeting belongs to.
///
/// A recording imported from Drive is keyed by its Drive file id; anything
/// else by its canonical source path. The output folder is never part of the
/// key: transcribing the same file again writes `{stem} 2`, and that must
/// still land in the same Drive folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingKey(String);

impl MeetingKey {
    pub fn drive(id: &DriveFileId) -> Self {
        let key = format!("drive:{}", id.as_str());
        if MEETING_PROPERTY.len() + key.len() <= APP_PROPERTY_BYTES {
            Self(key)
        } else {
            Self(format!("drive:{}", stable_hash(id.as_str().as_bytes())))
        }
    }

    pub fn local(source: &Path) -> Self {
        Self(format!(
            "local:{}",
            stable_hash(source.as_os_str().as_encoded_bytes())
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The key a Galpi upload carries in its `galpiMeeting` property, or
    /// `None` for anything Galpi would not have written: another prefix, a
    /// character outside a key's alphabet, or a value Drive could not store.
    pub fn from_property(value: &str) -> Option<Self> {
        let (prefix, id) = value.split_once(':')?;
        let valid = matches!(prefix, "local" | "drive")
            && !id.is_empty()
            && MEETING_PROPERTY.len() + value.len() <= APP_PROPERTY_BYTES
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        valid.then(|| Self(value.to_owned()))
    }
}

/// A meeting that can be synced: its key and the Drive folder name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveMeeting {
    pub key: MeetingKey,
    pub name: String,
}

impl DriveMeeting {
    /// Keyed as described on [`MeetingKey`], named after the source's stem.
    /// A speaker transcript is named after its recording: the folder of
    /// `새로운 노트_화자별.txt` is `새로운 노트`.
    pub fn new(key: MeetingKey, source: &Path) -> Self {
        let transcript = source
            .file_name()
            .is_some_and(|name| is_transcript_name(&name.to_string_lossy()));
        let name = source
            .file_stem()
            .map(|stem| {
                let stem = stem.to_string_lossy();
                let named = if transcript {
                    recording_name(stem.as_ref())
                } else {
                    stem.as_ref()
                };
                named.trim().to_owned()
            })
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| "meeting".to_owned());
        Self { key, name }
    }
}

/// A Galpi result file already on Drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    pub id: String,
    /// What Drive reports for the current content; absent for some files.
    pub sha256_checksum: Option<String>,
    /// The content checksum Galpi recorded (`galpiSha256`) on its last write.
    pub synced_sha256: Option<String>,
    pub web_view_link: Option<String>,
}

/// What one sync does with one result file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDecision {
    Create,
    Unchanged,
    Update,
    /// The Drive copy changed since Galpi last wrote it; leave it alone.
    Conflict,
}

/// Decide from the Drive copy alone, so no local sync record is needed.
///
/// Galpi records the checksum of what it uploaded in `galpiSha256`. When Drive
/// still reports that checksum, nobody else touched the file; when it reports
/// something else, or nothing at all, someone may have, and only an explicit
/// `overwrite` replaces it.
pub fn decide(remote: Option<&RemoteFile>, local_sha256: &str, overwrite: bool) -> SyncDecision {
    let Some(remote) = remote else {
        return SyncDecision::Create;
    };
    match (&remote.sha256_checksum, &remote.synced_sha256) {
        (Some(current), Some(synced)) if current == synced => {
            if current == local_sha256 {
                SyncDecision::Unchanged
            } else {
                SyncDecision::Update
            }
        }
        _ if overwrite => SyncDecision::Update,
        _ => SyncDecision::Conflict,
    }
}

/// How one result file fared in a sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum DriveSyncState {
    Uploaded,
    Updated,
    Unchanged,
    Conflict,
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveSyncFile {
    pub kind: ArtifactKind,
    #[serde(flatten)]
    pub state: DriveSyncState,
    pub web_view_link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveSyncResult {
    pub folder_link: Option<String>,
    pub files: Vec<DriveSyncFile>,
}

/// A recording downloaded from Drive, ready to transcribe from `input_path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveImport {
    pub job_id: Uuid,
    pub input_path: String,
    pub drive_file_id: DriveFileId,
    pub name: String,
}

/// How uploading one finished recording ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DriveRecordingState {
    Uploaded,
    /// The local file changed since Galpi last uploaded it.
    Updated,
    Unchanged,
    /// The Drive copy changed elsewhere; it was left alone.
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveRecordingUpload {
    pub state: DriveRecordingState,
    pub folder_link: Option<String>,
    pub web_view_link: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        APP_PROPERTY_BYTES, DriveClient, DriveClientError, DriveClientStatus, DriveFileId,
        DriveIdError, DriveMeeting, DriveRecordingState, DriveRecordingUpload, DriveStatus,
        DriveSyncFile, DriveSyncState, DriveTokens, MEETING_PROPERTY, MeetingKey, RemoteFile,
        SyncDecision, decide, parse_picked_ids, stable_hash,
    };
    use crate::domain::artifact::ArtifactKind;
    use serde_json::json;
    use std::path::Path;

    const LOCAL: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const SYNCED: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    const OTHER: &str = "3333333333333333333333333333333333333333333333333333333333333333";

    fn remote(current: Option<&str>, synced: Option<&str>) -> RemoteFile {
        RemoteFile {
            id: "file-1".to_owned(),
            sha256_checksum: current.map(str::to_owned),
            synced_sha256: synced.map(str::to_owned),
            web_view_link: None,
        }
    }

    #[test]
    fn a_missing_drive_copy_is_created() {
        assert_eq!(decide(None, LOCAL, false), SyncDecision::Create);
    }

    #[test]
    fn an_untouched_drive_copy_with_the_same_content_is_left_unchanged() {
        let untouched = remote(Some(LOCAL), Some(LOCAL));
        assert_eq!(
            decide(Some(&untouched), LOCAL, false),
            SyncDecision::Unchanged
        );
        assert_eq!(
            decide(Some(&untouched), LOCAL, true),
            SyncDecision::Unchanged
        );
    }

    #[test]
    fn an_untouched_drive_copy_is_updated_when_only_the_local_file_changed() {
        let untouched = remote(Some(SYNCED), Some(SYNCED));
        assert_eq!(decide(Some(&untouched), LOCAL, false), SyncDecision::Update);
    }

    #[test]
    fn a_drive_copy_changed_elsewhere_is_a_conflict_unless_overwritten() {
        let edited = remote(Some(OTHER), Some(SYNCED));
        assert_eq!(decide(Some(&edited), LOCAL, false), SyncDecision::Conflict);
        assert_eq!(decide(Some(&edited), LOCAL, true), SyncDecision::Update);
    }

    #[test]
    fn a_drive_copy_without_a_checksum_cannot_be_trusted() {
        let unknown = remote(None, Some(SYNCED));
        let unrecorded = remote(Some(SYNCED), None);
        assert_eq!(decide(Some(&unknown), LOCAL, false), SyncDecision::Conflict);
        assert_eq!(
            decide(Some(&unrecorded), LOCAL, false),
            SyncDecision::Conflict
        );
        assert_eq!(decide(Some(&unknown), LOCAL, true), SyncDecision::Update);
    }

    #[test]
    fn file_ids_accept_only_short_query_safe_values() {
        assert!(DriveFileId::parse("1AbC-d_9").is_ok());
        assert!(DriveFileId::parse(&"a".repeat(200)).is_ok());
        for invalid in ["", "a'b", "a/b", "a b", "a\\b"] {
            assert_eq!(DriveFileId::parse(invalid), Err(DriveIdError::Invalid));
        }
        assert_eq!(
            DriveFileId::parse(&"a".repeat(201)),
            Err(DriveIdError::Invalid)
        );
    }

    #[test]
    fn picked_ids_are_parsed_from_the_callback_list() -> Result<(), DriveIdError> {
        assert_eq!(parse_picked_ids("")?, Vec::new());
        let picked = parse_picked_ids("abc,def-1")?;
        assert_eq!(
            picked.iter().map(DriveFileId::as_str).collect::<Vec<_>>(),
            vec!["abc", "def-1"]
        );
        assert_eq!(parse_picked_ids("abc,../x"), Err(DriveIdError::Invalid));
        Ok(())
    }

    #[test]
    fn meeting_keys_fit_one_app_property() -> Result<(), DriveIdError> {
        let local = MeetingKey::local(Path::new("/Users/me/녹음/회의.m4a"));
        let drive = MeetingKey::drive(&DriveFileId::parse("1AbC")?);
        let long = MeetingKey::drive(&DriveFileId::parse(&"x".repeat(200))?);

        assert!(local.as_str().starts_with("local:"));
        assert_eq!(drive.as_str(), "drive:1AbC");
        assert!(long.as_str().starts_with("drive:"));
        for key in [&local, &drive, &long] {
            assert!(MEETING_PROPERTY.len() + key.as_str().len() <= APP_PROPERTY_BYTES);
        }
        assert_eq!(
            local,
            MeetingKey::local(Path::new("/Users/me/녹음/회의.m4a"))
        );
        assert_ne!(
            local,
            MeetingKey::local(Path::new("/Users/me/녹음/다른.m4a"))
        );
        Ok(())
    }

    #[test]
    fn a_meeting_is_named_after_its_source_stem() {
        let meeting = DriveMeeting::new(
            MeetingKey::local(Path::new("/tmp/주간 회의.m4a")),
            Path::new("/tmp/주간 회의.m4a"),
        );
        assert_eq!(meeting.name, "주간 회의");
    }

    #[test]
    fn a_speaker_transcript_names_the_meeting_after_its_recording() {
        // Given: the transcript Galpi wrote next to a recording named 새로운 노트
        let transcript = Path::new("/tmp/새로운 노트/새로운 노트_화자별.txt");

        // When
        let meeting = DriveMeeting::new(MeetingKey::local(transcript), transcript);

        // Then: no `_화자별` in the Drive folder name
        assert_eq!(meeting.name, "새로운 노트");
    }

    #[test]
    fn only_transcripts_lose_the_speaker_suffix() {
        let plain = DriveMeeting::new(
            MeetingKey::local(Path::new("/tmp/메모.txt")),
            Path::new("/tmp/메모.txt"),
        );
        let recording = DriveMeeting::new(
            MeetingKey::local(Path::new("/tmp/회의_화자별.m4a")),
            Path::new("/tmp/회의_화자별.m4a"),
        );

        assert_eq!(plain.name, "메모");
        assert_eq!(recording.name, "회의_화자별");
    }

    #[test]
    fn galpi_property_values_become_keys() -> Result<(), DriveIdError> {
        let long = format!("drive:{}", "x".repeat(200));

        assert_eq!(
            MeetingKey::from_property("drive:1AbC-d_9"),
            Some(MeetingKey::drive(&DriveFileId::parse("1AbC-d_9")?))
        );
        assert_eq!(
            MeetingKey::from_property("local:af63dc4c8601ec8c")
                .as_ref()
                .map(MeetingKey::as_str),
            Some("local:af63dc4c8601ec8c")
        );
        for ignored in [
            "",
            "local:",
            "other:abc",
            "abc",
            "drive:a b",
            "drive:a'b",
            "drive:../x",
            long.as_str(),
        ] {
            assert_eq!(MeetingKey::from_property(ignored), None, "{ignored}");
        }
        Ok(())
    }

    #[test]
    fn meeting_hashes_match_the_published_fnv_1a_vectors() {
        assert_eq!(stable_hash(b""), "cbf29ce484222325");
        assert_eq!(stable_hash(b"a"), "af63dc4c8601ec8c");
        assert_eq!(stable_hash(b"foobar"), "85944171f73967e8");
    }

    #[test]
    fn drive_tokens_debug_hides_every_token() {
        let tokens = DriveTokens {
            access_token: "galpi-test-access".to_owned(),
            refresh_token: "galpi-test-refresh".to_owned(),
            expires_at: 7,
        };

        let printed = format!("{tokens:?}");

        assert!(!printed.contains("galpi-test"));
        assert!(printed.contains("expires_at: 7"));
    }

    #[test]
    fn status_and_sync_results_serialize_for_the_window() -> Result<(), serde_json::Error> {
        let disconnected = DriveStatus::Disconnected {
            onboarding_dismissed: true,
        };
        let connected = DriveStatus::Connected {
            email: "user@example.com".to_owned(),
            save_recordings: true,
        };
        let failed = DriveSyncFile {
            kind: ArtifactKind::SpeakerText,
            state: DriveSyncState::Failed {
                message: "실패".to_owned(),
            },
            web_view_link: None,
        };
        let recording = DriveRecordingUpload {
            state: DriveRecordingState::Unchanged,
            folder_link: Some("https://drive.google.com/drive/folders/m1".to_owned()),
            web_view_link: None,
        };

        assert_eq!(
            serde_json::to_value(&disconnected)?,
            json!({"state": "disconnected", "onboardingDismissed": true})
        );
        assert_eq!(
            serde_json::to_value(&connected)?,
            json!({"state": "connected", "email": "user@example.com", "saveRecordings": true})
        );
        assert_eq!(
            serde_json::to_value(&failed)?,
            json!({"kind": "speaker_text", "state": "failed", "message": "실패", "webViewLink": null})
        );
        assert_eq!(
            serde_json::to_value(&recording)?,
            json!({
                "state": "unchanged",
                "folderLink": "https://drive.google.com/drive/folders/m1",
                "webViewLink": null,
            })
        );
        Ok(())
    }

    const CLIENT_ID: &str = "123-abc.apps.googleusercontent.com";

    #[test]
    fn drive_client_invalid_ids_are_refused() {
        // Given: ids that are not `<digits>-<a-z0-9>.apps.googleusercontent.com`
        let bad = [
            "",
            "123-abc",
            "abc-def.apps.googleusercontent.com",
            "123-.apps.googleusercontent.com",
            "-abc.apps.googleusercontent.com",
            "123-ABC.apps.googleusercontent.com",
            "123-a b.apps.googleusercontent.com",
            "123abc.apps.googleusercontent.com",
            "123-abc.apps.googleusercontent.org",
        ];

        // When / Then
        for id in bad {
            assert_eq!(
                DriveClient::parse(id, "test-client-secret").err(),
                Some(DriveClientError::Id),
                "{id:?}"
            );
        }
    }

    #[test]
    fn drive_client_invalid_secrets_are_refused() {
        // Given: an empty secret, one with whitespace inside, and one over 256 characters
        let too_long = "x".repeat(257);
        let bad = ["", "   ", "has space", "tab\tinside", too_long.as_str()];

        // When / Then
        for secret in bad {
            assert_eq!(
                DriveClient::parse(CLIENT_ID, secret).err(),
                Some(DriveClientError::Secret),
                "{secret:?}"
            );
        }
    }

    #[test]
    fn drive_client_invalid_input_is_trimmed_before_it_is_judged() -> Result<(), DriveClientError> {
        // Given: values pasted with surrounding whitespace, and a secret without the
        // GOCSPX- prefix that older clients have
        let client = DriveClient::parse(
            "  123-abc.apps.googleusercontent.com\n",
            " test-client-secret\t",
        )?;

        // Then
        assert_eq!(client.id(), CLIENT_ID);
        assert_eq!(client.secret(), "test-client-secret");
        Ok(())
    }

    #[test]
    fn drive_client_hidden_from_debug_output() -> Result<(), DriveClientError> {
        // Given
        let client = DriveClient::parse(CLIENT_ID, "test-client-secret")?;

        // When
        let shown = format!("{client:?}");

        // Then: the id may show, the secret never does
        assert!(shown.contains(CLIENT_ID));
        assert!(!shown.contains("test-client-secret"));
        Ok(())
    }

    #[test]
    fn drive_client_hidden_from_the_window_status() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let client = DriveClient::parse(CLIENT_ID, "test-client-secret")
            .map_err(|error| format!("{error:?}"))?;

        // When
        let saved = serde_json::to_value(DriveClientStatus::of(Some(&client)))?;
        let none = serde_json::to_value(DriveClientStatus::of(None))?;

        // Then: only `configured` and `clientId` cross the boundary
        assert_eq!(saved, json!({"configured": true, "clientId": CLIENT_ID}));
        assert_eq!(none, json!({"configured": false, "clientId": null}));
        Ok(())
    }
}
