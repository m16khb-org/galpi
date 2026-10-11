//! Drive v3 calls with the user's `drive.file` access token.
//!
//! Galpi marks what it creates with `appProperties` (`galpiRoot`,
//! `galpiMeeting`, `galpiKind`, `galpiSha256`) and finds meeting folders and
//! result files again by querying them; only the root folder id is kept with
//! the connection.

use super::super::paths::{AppPaths, canonical, sanitized_stem};
use super::Tuning;
use crate::application::error::AppError;
use crate::application::ports::{DriveFilesPort, LocalUpload, PickedFile, RemoteFolder};
use crate::domain::artifact::ArtifactKind;
use crate::domain::drive::{DriveAccount, DriveFileId, MEETING_PROPERTY, MeetingKey, RemoteFile};
use async_trait::async_trait;
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, RequestBuilder, Response, StatusCode, Url};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;
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
    }
}

const fn mime_type(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Srt => "application/x-subrip",
        ArtifactKind::SpeakerText => "text/plain",
        ArtifactKind::Checkpoint => "application/json",
        ArtifactKind::Minutes => "text/markdown",
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

pub struct DriveRestAdapter {
    http: Client,
    api: String,
    upload: String,
    /// `cache/drive-imports`: one fresh folder per download.
    imports: PathBuf,
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
                "name,mimeType,capabilities(canDownload),modifiedTime",
            )],
        )?;
        let body: FileBody = parse(&self.call(self.http.get(url), access_token).await?)?;
        Ok(PickedFile {
            name: body.name.unwrap_or_default(),
            mime_type: body.mime_type.unwrap_or_default(),
            can_download: body
                .capabilities
                .is_some_and(|capabilities| capabilities.can_download),
            modified_time: body.modified_time,
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
