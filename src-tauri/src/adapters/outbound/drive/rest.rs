//! Drive v3 calls with the user's `drive.file` access token.
//!
//! Galpi marks what it creates with `appProperties` (`galpiRoot`,
//! `galpiMeeting`, `galpiKind`, `galpiSha256`) and finds meeting folders and
//! result files again by querying them; only the root folder id is kept with
//! the connection.

use super::super::paths::{AppPaths, canonical, sanitized_stem};
use super::Tuning;
use crate::application::error::AppError;
use crate::application::ports::{
    DriveFilesPort, LargeUpload, LocalFileDigest, LocalUpload, PickedFile, RemoteFolder,
};
use crate::domain::artifact::ArtifactKind;
use crate::domain::drive::{DriveAccount, DriveFileId, MEETING_PROPERTY, MeetingKey, RemoteFile};
use async_trait::async_trait;
use reqwest::header::{CONTENT_RANGE, CONTENT_TYPE, LOCATION, RANGE};
use reqwest::{Client, RequestBuilder, Response, StatusCode, Url};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::oneshot;
use tokio::time::timeout;
use uuid::Uuid;

/// Lowercase hex SHA-256, the form Drive reports in `sha256Checksum`.
fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const ROOT_FOLDER_NAME: &str = "Galpi";
const ROOT_PROPERTY: &str = "galpiRoot";
const KIND_PROPERTY: &str = "galpiKind";
const SHA_PROPERTY: &str = "galpiSha256";
/// Drive's limit for a single multipart upload.
const MAX_UPLOAD_BYTES: u64 = 5 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// A download may take as long as it needs, but neither its first answer nor
/// any later chunk may stall longer than this.
const CHUNK_TIMEOUT: Duration = Duration::from_secs(60);
/// Drive wants every chunk of a resumable upload but the last to be a multiple of this.
const UPLOAD_CHUNK_ALIGNMENT: u64 = 256 * 1024;
/// How much of a local file is read at a time while hashing it.
const DIGEST_BUFFER_BYTES: usize = 1024 * 1024;

fn unavailable() -> AppError {
    AppError::new(
        "DRIVE_UNAVAILABLE",
        "Google Drive에 연결하지 못했습니다. 잠시 뒤 다시 시도해 주세요.",
    )
}

fn unreadable() -> AppError {
    AppError::new(
        "DRIVE_REQUEST_FAILED",
        "Google Drive 응답 형식을 알 수 없습니다.",
    )
}

fn cancelled() -> AppError {
    AppError::new("CANCELLED", "사용자가 작업을 취소했습니다.")
}

fn read_failed(error: &std::io::Error) -> AppError {
    AppError::io("올릴 녹음 파일을 읽지 못했습니다", error)
}

fn changed_during_upload() -> AppError {
    AppError::new(
        "DRIVE_UPLOAD_CHANGED",
        "녹음 파일이 올리는 도중 바뀌어 Drive에 올리지 않았습니다.",
    )
}

fn download_failed() -> AppError {
    AppError::new(
        "DRIVE_DOWNLOAD_FAILED",
        "Drive에서 이 파일을 내려받지 못했습니다.",
    )
}

/// `{"error": {"errors": [{"reason": ".."}]}}`
#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    #[serde(default)]
    errors: Vec<ErrorReason>,
}

#[derive(Deserialize)]
struct ErrorReason {
    #[serde(default)]
    reason: String,
}

fn reasons(body: &[u8]) -> Vec<String> {
    serde_json::from_slice::<ErrorEnvelope>(body)
        .map(|envelope| {
            envelope
                .error
                .errors
                .into_iter()
                .map(|error| error.reason)
                .collect()
        })
        .unwrap_or_default()
}

/// Map a Drive error answer to a stable code. Never retried here.
fn api_error(status: StatusCode, body: &[u8]) -> AppError {
    let reasons = reasons(body);
    let has = |reason: &str| reasons.iter().any(|candidate| candidate == reason);
    if status == StatusCode::UNAUTHORIZED {
        return AppError::new(
            "DRIVE_TOKEN_REJECTED",
            "Google Drive가 연결을 받아들이지 않았습니다.",
        );
    }
    if status == StatusCode::FORBIDDEN && has("storageQuotaExceeded") {
        return AppError::new(
            "DRIVE_STORAGE_FULL",
            "Google Drive 저장 공간이 부족해 올리지 못했습니다.",
        );
    }
    if status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
        || (status == StatusCode::FORBIDDEN
            && (has("rateLimitExceeded") || has("userRateLimitExceeded")))
    {
        return unavailable();
    }
    if status == StatusCode::NOT_FOUND {
        return AppError::new(
            "DRIVE_NOT_FOUND",
            "Google Drive에서 파일을 찾지 못했습니다.",
        );
    }
    AppError::new(
        "DRIVE_REQUEST_FAILED",
        "Google Drive가 요청을 처리하지 못했습니다.",
    )
}

fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, AppError> {
    serde_json::from_slice(bytes).map_err(|_| unreadable())
}

/// Escape a value for a single-quoted Drive `q` string.
fn quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn has_property(key: &str, value: &str) -> String {
    format!(
        "appProperties has {{ key='{}' and value='{}' }}",
        quote(key),
        quote(value)
    )
}

const fn kind_name(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Srt => "srt",
        ArtifactKind::SpeakerText => "speaker_text",
        ArtifactKind::Checkpoint => "checkpoint",
        ArtifactKind::Minutes => "minutes",
        ArtifactKind::Recording => "recording",
    }
}

const fn mime_type(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Srt => "application/x-subrip",
        ArtifactKind::SpeakerText => "text/plain",
        ArtifactKind::Checkpoint => "application/json",
        ArtifactKind::Minutes => "text/markdown",
        ArtifactKind::Recording => "audio/wav",
    }
}

fn file_properties(key: &MeetingKey, kind: ArtifactKind, sha256: &str) -> Value {
    json!({
        MEETING_PROPERTY: key.as_str(),
        KIND_PROPERTY: kind_name(kind),
        SHA_PROPERTY: sha256,
    })
}

/// A `multipart/related` body: JSON metadata, then the content.
fn multipart(metadata: &Value, mime: &str, content: &[u8]) -> (String, Vec<u8>) {
    let boundary = format!("galpi-{}", Uuid::new_v4().simple());
    let mut body = Vec::with_capacity(content.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata}\r\n\
--{boundary}\r\nContent-Type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{boundary}--").as_bytes());
    (format!("multipart/related; boundary={boundary}"), body)
}

/// A file name safe on every platform, keeping the extension. Drive names are
/// plain strings, so they are never parsed as host paths: both `/` and `\`
/// separate segments on every OS, and a Windows drive prefix is ordinary text.
fn local_name(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty() && *segment != ".")
        .filter(|segment| *segment != "..")
        .unwrap_or_default();
    let (stem, extension) = match base.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, extension),
        _ => (base, ""),
    };
    let stem = sanitized_stem(stem);
    if !extension.is_empty() && extension.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        format!("{stem}.{extension}")
    } else {
        stem
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileBody {
    id: Option<String>,
    #[serde(default)]
    trashed: bool,
    web_view_link: Option<String>,
    sha256_checksum: Option<String>,
    #[serde(default)]
    app_properties: HashMap<String, String>,
    name: Option<String>,
    mime_type: Option<String>,
    capabilities: Option<Capabilities>,
    modified_time: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    #[serde(default)]
    can_download: bool,
}

#[derive(Deserialize)]
struct FileList {
    #[serde(default)]
    files: Vec<FileBody>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AboutBody {
    user: AboutUser,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AboutUser {
    email_address: String,
    permission_id: String,
}

fn required_id(body: FileBody) -> Result<String, AppError> {
    body.id.filter(|id| !id.is_empty()).ok_or_else(unreadable)
}

fn remote_file(mut body: FileBody, synced_sha256: Option<String>) -> Result<RemoteFile, AppError> {
    let synced_sha256 = synced_sha256.or_else(|| body.app_properties.remove(SHA_PROPERTY));
    Ok(RemoteFile {
        sha256_checksum: body.sha256_checksum.take(),
        web_view_link: body.web_view_link.take(),
        synced_sha256,
        id: required_id(body)?,
    })
}

/// Drive's answer to one chunk of a resumable upload.
enum ChunkAnswer {
    /// Drive keeps the file up to this offset and waits for the rest.
    Resume(u64),
    /// The upload is complete; this is the file's metadata.
    Done(Vec<u8>),
}

/// The byte count a `Range: bytes=0-N` header says Drive holds.
fn persisted_bytes(range: &str) -> Option<u64> {
    range
        .strip_prefix("bytes=0-")?
        .parse::<u64>()
        .ok()?
        .checked_add(1)
}

pub struct DriveRestAdapter {
    http: Client,
    api: String,
    upload: String,
    /// `cache/drive-imports`: one fresh folder per download.
    imports: PathBuf,
    /// Size of every chunk of a resumable upload but the last.
    upload_chunk: u64,
}

impl DriveRestAdapter {
    pub fn new(app: &AppHandle) -> Result<Self, AppError> {
        let imports = AppPaths::resolve(app)?.cache.join("drive-imports");
        Self::with_tuning(&Tuning::production(), imports)
    }

    pub(super) fn with_tuning(tuning: &Tuning, imports: PathBuf) -> Result<Self, AppError> {
        // No overall timeout on the client: a download takes as long as the
        // file needs. Every other request sets its own.
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| unavailable())?;
        Ok(Self {
            http,
            api: tuning.drive_api_url.clone(),
            upload: tuning.drive_upload_url.clone(),
            imports,
            upload_chunk: (tuning.upload_chunk_bytes / UPLOAD_CHUNK_ALIGNMENT).max(1)
                * UPLOAD_CHUNK_ALIGNMENT,
        })
    }

    /// Remove every leftover import; called once at startup.
    pub fn clear_imports(&self) -> std::io::Result<()> {
        match std::fs::remove_dir_all(&self.imports) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    fn url(base: &str, path: &str, query: &[(&str, &str)]) -> Result<Url, AppError> {
        let mut url = Url::parse(&format!("{base}{path}")).map_err(|_| unreadable())?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        Ok(url)
    }

    /// Send with the request timeout and return the body of a success.
    async fn call(&self, request: RequestBuilder, access_token: &str) -> Result<Vec<u8>, AppError> {
        let response = request
            .bearer_auth(access_token)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|_| unavailable())?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|_| unavailable())?;
        if status.is_success() {
            Ok(bytes.to_vec())
        } else {
            Err(api_error(status, &bytes))
        }
    }

    async fn list(
        &self,
        access_token: &str,
        q: &str,
        fields: &str,
    ) -> Result<Vec<FileBody>, AppError> {
        let url = Self::url(
            &self.api,
            "/files",
            &[
                ("q", q),
                ("fields", fields),
                ("spaces", "drive"),
                ("pageSize", "10"),
            ],
        )?;
        let bytes = self.call(self.http.get(url), access_token).await?;
        Ok(parse::<FileList>(&bytes)?.files)
    }

    async fn create_folder(
        &self,
        access_token: &str,
        metadata: &Value,
    ) -> Result<FileBody, AppError> {
        let url = Self::url(&self.api, "/files", &[("fields", "id,webViewLink")])?;
        let request = self
            .http
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .body(metadata.to_string());
        parse(&self.call(request, access_token).await?)
    }

    /// Whether `id` still names a folder Galpi may use.
    async fn usable_folder(&self, access_token: &str, id: &str) -> Result<bool, AppError> {
        let Ok(id) = DriveFileId::parse(id) else {
            return Ok(false);
        };
        let url = Self::url(
            &self.api,
            &format!("/files/{}", id.as_str()),
            &[("fields", "id,trashed")],
        )?;
        match self.call(self.http.get(url), access_token).await {
            Ok(bytes) => Ok(!parse::<FileBody>(&bytes)?.trashed),
            Err(error) if error.code == "DRIVE_NOT_FOUND" => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Open a resumable session for `upload` and return its URI.
    async fn open_session(
        &self,
        access_token: &str,
        upload: &LargeUpload<'_>,
    ) -> Result<String, AppError> {
        let query = [
            ("uploadType", "resumable"),
            ("fields", "id,webViewLink,sha256Checksum"),
        ];
        let properties = file_properties(upload.key, upload.kind, &upload.digest.sha256);
        let (request, metadata) = if let Some(id) = upload.existing {
            let id = DriveFileId::parse(id).map_err(|_| unreadable())?;
            let url = Self::url(&self.upload, &format!("/files/{}", id.as_str()), &query)?;
            (self.http.patch(url), json!({"appProperties": properties}))
        } else {
            let url = Self::url(&self.upload, "/files", &query)?;
            let metadata = json!({
                "name": upload.digest.name,
                "parents": [upload.folder],
                "mimeType": mime_type(upload.kind),
                "appProperties": properties,
            });
            (self.http.post(url), metadata)
        };
        let response = request
            .bearer_auth(access_token)
            .timeout(REQUEST_TIMEOUT)
            .header(CONTENT_TYPE, "application/json; charset=UTF-8")
            .header("X-Upload-Content-Type", mime_type(upload.kind))
            .header("X-Upload-Content-Length", upload.digest.size)
            .body(metadata.to_string())
            .send()
            .await
            .map_err(|_| unavailable())?;
        let status = response.status();
        if !status.is_success() {
            let body = response.bytes().await.unwrap_or_default();
            return Err(api_error(status, &body));
        }
        response
            .headers()
            .get(LOCATION)
            .and_then(|location| location.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(unreadable)
    }

    /// Send one chunk to the session. The session URI itself authorizes the
    /// request, so the access token never goes to the address Drive named.
    async fn put_chunk(
        &self,
        session: &str,
        content_range: &str,
        chunk: Vec<u8>,
    ) -> Result<ChunkAnswer, AppError> {
        let response = self
            .http
            .put(session)
            .timeout(CHUNK_TIMEOUT)
            .header(CONTENT_RANGE, content_range)
            .body(chunk)
            .send()
            .await
            .map_err(|_| unavailable())?;
        let status = response.status();
        let held = response
            .headers()
            .get(RANGE)
            .and_then(|range| range.to_str().ok())
            .and_then(persisted_bytes);
        let body = response.bytes().await.map_err(|_| unavailable())?;
        if status == StatusCode::PERMANENT_REDIRECT {
            Ok(ChunkAnswer::Resume(held.unwrap_or(0)))
        } else if status.is_success() {
            Ok(ChunkAnswer::Done(body.to_vec()))
        } else {
            Err(api_error(status, &body))
        }
    }

    /// Stream the file to the session in chunks and return the finished
    /// file's metadata. The file is hashed once more on the way; the last
    /// chunk, which completes the file on Drive, is sent only when the hash
    /// still matches the one the checksum property was built from.
    async fn stream_file(
        &self,
        session: &str,
        upload: &LargeUpload<'_>,
    ) -> Result<Vec<u8>, AppError> {
        let size = upload.digest.size;
        let mut file = tokio::fs::File::open(upload.path)
            .await
            .map_err(|error| AppError::io("올릴 녹음 파일을 열지 못했습니다", &error))?;
        let mut hasher = Sha256::new();
        let mut hashed_until = 0_u64;
        let mut offset = 0_u64;
        loop {
            let end = offset.saturating_add(self.upload_chunk).min(size);
            let length = usize::try_from(end - offset).map_err(|_| unavailable())?;
            file.seek(SeekFrom::Start(offset))
                .await
                .map_err(|error| read_failed(&error))?;
            let mut chunk = vec![0_u8; length];
            file.read_exact(&mut chunk)
                .await
                .map_err(|error| read_failed(&error))?;
            // Bytes sent again after a partial answer were hashed the first time.
            let seen = usize::try_from(hashed_until.saturating_sub(offset))
                .map_or(length, |seen| seen.min(length));
            hasher.update(&chunk[seen..]);
            hashed_until = hashed_until.max(end);
            if end == size && format!("{:x}", hasher.clone().finalize()) != upload.digest.sha256 {
                return Err(changed_during_upload());
            }
            let content_range = if size == 0 {
                "bytes */0".to_owned()
            } else {
                format!("bytes {offset}-{}/{size}", end - 1)
            };
            match self.put_chunk(session, &content_range, chunk).await? {
                ChunkAnswer::Done(body) => return Ok(body),
                // Drive must keep part of what it was just sent, but never more
                // than that and never the whole file without finishing it.
                ChunkAnswer::Resume(held) if held > offset && held <= end && held < size => {
                    offset = held;
                }
                ChunkAnswer::Resume(_) => return Err(unavailable()),
            }
        }
    }

    async fn send_file(
        &self,
        request: RequestBuilder,
        access_token: &str,
        metadata: &Value,
        kind: ArtifactKind,
        upload: &LocalUpload,
    ) -> Result<RemoteFile, AppError> {
        let (content_type, body) = multipart(metadata, mime_type(kind), &upload.content);
        let request = request.header(CONTENT_TYPE, content_type).body(body);
        let bytes = self.call(request, access_token).await?;
        remote_file(parse(&bytes)?, Some(upload.sha256.clone()))
    }

    /// Stream `id` into `folder/name` through a `.part` file.
    async fn stream(
        &self,
        access_token: &str,
        id: &DriveFileId,
        folder: &Path,
        name: &str,
        modified: Option<SystemTime>,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<PathBuf, AppError> {
        let url = Self::url(
            &self.api,
            &format!("/files/{}", id.as_str()),
            &[("alt", "media")],
        )?;
        let request = self.http.get(url).bearer_auth(access_token).send();
        let mut response: Response = tokio::select! {
            biased;
            _ = &mut *cancel => return Err(cancelled()),
            sent = timeout(CHUNK_TIMEOUT, request) => {
                sent.map_err(|_| unavailable())?.map_err(|_| unavailable())?
            }
        };
        let status = response.status();
        if !status.is_success() {
            let body = response.bytes().await.unwrap_or_default();
            let error = api_error(status, &body);
            return Err(match error.code.as_str() {
                "DRIVE_TOKEN_REJECTED" | "DRIVE_UNAVAILABLE" => error,
                _ => download_failed(),
            });
        }
        let part = folder.join(format!("{name}.part"));
        let mut file = tokio::fs::File::create(&part)
            .await
            .map_err(|error| AppError::io("내려받을 파일을 만들지 못했습니다", &error))?;
        loop {
            let chunk = tokio::select! {
                biased;
                _ = &mut *cancel => return Err(cancelled()),
                chunk = timeout(CHUNK_TIMEOUT, response.chunk()) => chunk,
            };
            match chunk {
                Ok(Ok(Some(bytes))) => file
                    .write_all(&bytes)
                    .await
                    .map_err(|error| AppError::io("내려받은 내용을 저장하지 못했습니다", &error))?,
                Ok(Ok(None)) => break,
                Ok(Err(_)) | Err(_) => return Err(unavailable()),
            }
        }
        file.flush()
            .await
            .map_err(|error| AppError::io("내려받은 내용을 저장하지 못했습니다", &error))?;
        let file = file.into_std().await;
        // Refinement dates the minutes from the recording's modification
        // time, so the copy keeps the Drive file's rather than today's.
        if let Some(modified) = modified {
            file.set_modified(modified)
                .map_err(|error| AppError::io("내려받은 파일 시각을 맞추지 못했습니다", &error))?;
        }
        drop(file);
        let destination = folder.join(name);
        tokio::fs::rename(&part, &destination)
            .await
            .map_err(|error| AppError::io("내려받은 파일을 저장하지 못했습니다", &error))?;
        canonical(&destination)
            .await
            .map_err(|error| AppError::io("내려받은 파일을 확인하지 못했습니다", &error))
    }
}

#[async_trait]
impl DriveFilesPort for DriveRestAdapter {
    async fn whoami(&self, access_token: &str) -> Result<DriveAccount, AppError> {
        let url = Self::url(
            &self.api,
            "/about",
            &[("fields", "user(emailAddress,permissionId)")],
        )?;
        let about: AboutBody = parse(&self.call(self.http.get(url), access_token).await?)?;
        Ok(DriveAccount {
            email: about.user.email_address,
            permission_id: about.user.permission_id,
        })
    }

    async fn ensure_root_folder(
        &self,
        access_token: &str,
        known: Option<&str>,
    ) -> Result<String, AppError> {
        if let Some(known) = known
            && self.usable_folder(access_token, known).await?
        {
            return Ok(known.to_owned());
        }
        let q = format!(
            "mimeType='{FOLDER_MIME}' and trashed=false and {}",
            has_property(ROOT_PROPERTY, "1")
        );
        if let Some(found) = self
            .list(access_token, &q, "files(id)")
            .await?
            .into_iter()
            .next()
        {
            return required_id(found);
        }
        let created = self
            .create_folder(
                access_token,
                &json!({
                    "name": ROOT_FOLDER_NAME,
                    "mimeType": FOLDER_MIME,
                    "appProperties": {ROOT_PROPERTY: "1"},
                }),
            )
            .await?;
        required_id(created)
    }

    async fn ensure_meeting_folder(
        &self,
        access_token: &str,
        root: &str,
        key: &MeetingKey,
        name: &str,
    ) -> Result<RemoteFolder, AppError> {
        let q = format!(
            "mimeType='{FOLDER_MIME}' and '{}' in parents and trashed=false and {}",
            quote(root),
            has_property(MEETING_PROPERTY, key.as_str())
        );
        let found = self
            .list(access_token, &q, "files(id,webViewLink)")
            .await?
            .into_iter()
            .next();
        let mut folder = if let Some(folder) = found {
            folder
        } else {
            self.create_folder(
                access_token,
                &json!({
                    "name": name,
                    "mimeType": FOLDER_MIME,
                    "parents": [root],
                    "appProperties": {MEETING_PROPERTY: key.as_str()},
                }),
            )
            .await?
        };
        let link = folder.web_view_link.take();
        Ok(RemoteFolder {
            id: required_id(folder)?,
            link,
        })
    }

    async fn find_artifact(
        &self,
        access_token: &str,
        folder: &str,
        kind: ArtifactKind,
    ) -> Result<Option<RemoteFile>, AppError> {
        let q = format!(
            "'{}' in parents and trashed=false and {}",
            quote(folder),
            has_property(KIND_PROPERTY, kind_name(kind))
        );
        self.list(
            access_token,
            &q,
            "files(id,sha256Checksum,appProperties,webViewLink)",
        )
        .await?
        .into_iter()
        .next()
        .map(|file| remote_file(file, None))
        .transpose()
    }

    async fn read_local(&self, path: &Path) -> Result<LocalUpload, AppError> {
        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|error| AppError::io("올릴 결과 파일을 확인하지 못했습니다", &error))?;
        if metadata.len() > MAX_UPLOAD_BYTES {
            return Err(AppError::new(
                "DRIVE_UPLOAD_TOO_LARGE",
                "5MB가 넘는 파일은 Drive에 올리지 않습니다.",
            ));
        }
        let content = tokio::fs::read(path)
            .await
            .map_err(|error| AppError::io("올릴 결과 파일을 읽지 못했습니다", &error))?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| AppError::new("DRIVE_REQUEST_FAILED", "결과 파일 이름이 없습니다."))?;
        Ok(LocalUpload {
            name,
            sha256: sha256_hex(&content),
            content,
        })
    }

    async fn digest_local(&self, path: &Path) -> Result<LocalFileDigest, AppError> {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| AppError::new("DRIVE_REQUEST_FAILED", "녹음 파일 이름이 없습니다."))?;
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|error| AppError::io("올릴 녹음 파일을 열지 못했습니다", &error))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; DIGEST_BUFFER_BYTES];
        let mut size = 0_u64;
        loop {
            let read = file
                .read(&mut buffer)
                .await
                .map_err(|error| read_failed(&error))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            size = size.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        }
        Ok(LocalFileDigest {
            name,
            size,
            sha256: format!("{:x}", hasher.finalize()),
        })
    }

    async fn upload_large(
        &self,
        access_token: &str,
        upload: &LargeUpload<'_>,
    ) -> Result<RemoteFile, AppError> {
        let session = self.open_session(access_token, upload).await?;
        let finished = self.stream_file(&session, upload).await?;
        remote_file(parse(&finished)?, Some(upload.digest.sha256.clone()))
    }

    async fn create_file(
        &self,
        access_token: &str,
        folder: &str,
        key: &MeetingKey,
        kind: ArtifactKind,
        upload: &LocalUpload,
    ) -> Result<RemoteFile, AppError> {
        let url = Self::url(
            &self.upload,
            "/files",
            &[
                ("uploadType", "multipart"),
                ("fields", "id,webViewLink,sha256Checksum"),
            ],
        )?;
        let metadata = json!({
            "name": upload.name,
            "parents": [folder],
            "mimeType": mime_type(kind),
            "appProperties": file_properties(key, kind, &upload.sha256),
        });
        self.send_file(self.http.post(url), access_token, &metadata, kind, upload)
            .await
    }

    async fn update_file(
        &self,
        access_token: &str,
        file_id: &str,
        key: &MeetingKey,
        kind: ArtifactKind,
        upload: &LocalUpload,
    ) -> Result<RemoteFile, AppError> {
        let id = DriveFileId::parse(file_id).map_err(|_| unreadable())?;
        let url = Self::url(
            &self.upload,
            &format!("/files/{}", id.as_str()),
            &[
                ("uploadType", "multipart"),
                ("fields", "id,webViewLink,sha256Checksum"),
            ],
        )?;
        let metadata = json!({"appProperties": file_properties(key, kind, &upload.sha256)});
        self.send_file(self.http.patch(url), access_token, &metadata, kind, upload)
            .await
    }

    async fn file_metadata(
        &self,
        access_token: &str,
        id: &DriveFileId,
    ) -> Result<PickedFile, AppError> {
        let url = Self::url(
            &self.api,
            &format!("/files/{}", id.as_str()),
            &[(
                "fields",
                "name,mimeType,capabilities(canDownload),modifiedTime,appProperties",
            )],
        )?;
        let mut body: FileBody = parse(&self.call(self.http.get(url), access_token).await?)?;
        let meeting = body
            .app_properties
            .remove(MEETING_PROPERTY)
            .and_then(|value| MeetingKey::from_property(&value));
        Ok(PickedFile {
            name: body.name.unwrap_or_default(),
            mime_type: body.mime_type.unwrap_or_default(),
            can_download: body
                .capabilities
                .is_some_and(|capabilities| capabilities.can_download),
            modified_time: body.modified_time,
            meeting,
        })
    }

    async fn download(
        &self,
        access_token: &str,
        id: &DriveFileId,
        file: &PickedFile,
        cancel: &mut oneshot::Receiver<()>,
    ) -> Result<PathBuf, AppError> {
        let folder = self.imports.join(Uuid::new_v4().to_string());
        tokio::fs::create_dir_all(&folder)
            .await
            .map_err(|error| AppError::io("내려받을 폴더를 만들지 못했습니다", &error))?;
        let modified = file
            .modified_time
            .as_deref()
            .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
            .map(SystemTime::from);
        let result = self
            .stream(
                access_token,
                id,
                &folder,
                &local_name(&file.name),
                modified,
                cancel,
            )
            .await;
        if result.is_err() {
            // Nothing half-downloaded stays behind; startup clears any rest.
            let _removed = tokio::fs::remove_dir_all(&folder).await;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{local_name, multipart, quote, sha256_hex};
    use serde_json::json;

    #[test]
    fn query_values_escape_quotes_and_backslashes() {
        assert_eq!(quote(r"it's a\b"), r"it\'s a\\b");
    }

    #[test]
    fn checksums_are_lowercase_hex_like_drive_reports_them() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn downloaded_names_are_sanitized_and_keep_the_extension() {
        assert_eq!(local_name("주간 회의.m4a"), "주간 회의.m4a");
        assert_eq!(local_name("../../etc/passwd"), "passwd");
        assert_eq!(local_name("a:b?.WAV"), "a-b.WAV");
        assert_eq!(local_name(".m4a"), "m4a");
        assert_eq!(local_name("녹음.m4a/.."), "meeting");
        // Windows path syntax must not change the result on any OS.
        assert_eq!(local_name(r"C:\x\y.m4a"), "y.m4a");
        assert_eq!(local_name(r"..\..\boot.ini"), "boot.ini");
        assert_eq!(local_name("C:"), "C");
        assert_eq!(local_name("c:.wav"), "c.wav");
    }

    #[test]
    fn multipart_bodies_carry_metadata_then_content() {
        let (content_type, body) = multipart(&json!({"name": "a.txt"}), "text/plain", b"hello");
        let boundary = content_type
            .strip_prefix("multipart/related; boundary=")
            .unwrap_or_default();
        let body = String::from_utf8_lossy(&body);

        assert_ne!(boundary, "");
        assert!(body.starts_with(&format!("--{boundary}\r\nContent-Type: application/json")));
        assert!(body.contains("{\"name\":\"a.txt\"}"));
        assert!(body.contains("Content-Type: text/plain\r\n\r\nhello\r\n"));
        assert!(body.ends_with(&format!("--{boundary}--")));
    }
}
