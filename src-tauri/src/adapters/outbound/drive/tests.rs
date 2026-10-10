use super::{DriveOAuthAdapter, DriveRestAdapter, Tuning};
use crate::adapters::outbound::browser_sign_in::fake_server::{FakeServer, Handler, Recorded};
use crate::adapters::outbound::browser_sign_in::pkce::code_challenge;
use crate::adapters::outbound::browser_sign_in::{BrowserOpener, SignInCodes};
use crate::application::error::AppError;
use crate::application::ports::{
    ClockPort, DriveAuthPort, DriveAuthorize, DriveCallback, DriveFilesPort, LocalUpload,
    PickedFile,
};
use crate::domain::artifact::ArtifactKind;
use crate::domain::drive::{DriveFileId, DriveTokenFailure, DriveTokens, MeetingKey};
use reqwest::Url;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::oneshot;
use uuid::Uuid;

const NOW: u64 = 1_000_000;

/// What the browser "user" sends back, given the `state` of the request.
type Behavior = Box<dyn Fn(&str) -> String + Send + Sync>;

struct FakeBrowser {
    behavior: Behavior,
    urls: Mutex<Vec<String>>,
}

impl BrowserOpener for FakeBrowser {
    fn open(&self, url: &str, _codes: SignInCodes) -> Result<(), AppError> {
        let parsed = Url::parse(url).map_err(|_| AppError::new("TEST", "bad url"))?;
        let param = |name: &str| query(&parsed, name).unwrap_or_default();
        if let Ok(mut urls) = self.urls.lock() {
            urls.push(url.to_owned());
        }
        let target = format!(
            "{}?{}",
            param("redirect_uri"),
            (self.behavior)(&param("state"))
        );
        tokio::spawn(async move {
            let _response = reqwest::get(target).await;
        });
        Ok(())
    }
}

struct FixedClock;

impl ClockPort for FixedClock {
    fn now_unix(&self) -> u64 {
        NOW
    }
}

fn query(url: &Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

fn json_body(request: &Recorded) -> Value {
    serde_json::from_str(&request.body).unwrap_or(Value::Null)
}

fn tuning(base: &str) -> Tuning {
    Tuning {
        gateway_url: base.to_owned(),
        google_auth_url: format!("{base}/o/oauth2/v2/auth"),
        google_revoke_url: format!("{base}/revoke"),
        drive_api_url: format!("{base}/drive/v3"),
        drive_upload_url: format!("{base}/upload/drive/v3"),
        preferred_port: 0,
        sign_in_timeout: Duration::from_secs(30),
    }
}

fn not_found() -> (u16, String) {
    (
        404,
        json!({"error": {"code": 404, "errors": [{"reason": "notFound"}]}}).to_string(),
    )
}

struct AuthHarness {
    server: FakeServer,
    browser: Arc<FakeBrowser>,
    adapter: DriveOAuthAdapter,
}

impl AuthHarness {
    async fn start(behavior: Behavior, handler: Handler) -> Result<Self, AppError> {
        let server = FakeServer::start(handler)
            .await
            .map_err(|error| AppError::io("server", &error))?;
        let browser = Arc::new(FakeBrowser {
            behavior,
            urls: Mutex::new(Vec::new()),
        });
        let adapter = DriveOAuthAdapter::with_tuning(
            tuning(&server.base),
            Arc::clone(&browser) as Arc<dyn BrowserOpener>,
            Arc::new(FixedClock),
        )?;
        Ok(Self {
            server,
            browser,
            adapter,
        })
    }

    fn opened_url(&self) -> Result<Url, AppError> {
        let urls = self
            .browser
            .urls
            .lock()
            .map_err(|_| AppError::new("TEST", "urls lock"))?;
        let url = urls
            .first()
            .ok_or_else(|| AppError::new("TEST", "no browser was opened"))?;
        Url::parse(url).map_err(|_| AppError::new("TEST", "bad url"))
    }

    async fn authorize(&self, request: &DriveAuthorize) -> Result<DriveCallback, AppError> {
        let (_cancel, mut receiver) = oneshot::channel();
        self.adapter.authorize(request, &mut receiver).await
    }
}

fn answer(extra: &'static str) -> Behavior {
    Box::new(move |state| format!("state={state}{extra}"))
}

fn no_server() -> Handler {
    Arc::new(|_request| (404, String::new()))
}

const CONNECT: DriveAuthorize = DriveAuthorize {
    picker: false,
    login_hint: None,
};

fn callback() -> DriveCallback {
    DriveCallback {
        code: "galpi-test-code".to_owned(),
        verifier: "galpi-test-verifier".to_owned(),
        redirect_uri: "http://127.0.0.1:1455/auth/callback".to_owned(),
        picked: Vec::new(),
    }
}

fn token_answer(refresh: Option<&str>) -> String {
    let mut body = json!({
        "access_token": "galpi-test-access",
        "expires_in": 3599,
        "scope": "https://www.googleapis.com/auth/drive.file",
        "token_type": "Bearer",
    });
    if let Some(refresh) = refresh {
        body["refresh_token"] = json!(refresh);
    }
    body.to_string()
}

fn fixed(status: u16, body: String) -> Handler {
    Arc::new(move |_request| (status, body.clone()))
}

#[tokio::test]
async fn connecting_asks_google_for_drive_file_with_pkce_and_offline_access() -> Result<(), AppError>
{
    // Given
    let harness = AuthHarness::start(answer("&code=galpi-test-code"), no_server()).await?;

    // When
    let callback = harness.authorize(&CONNECT).await?;

    // Then
    let url = harness.opened_url()?;
    assert_eq!(url.path(), "/o/oauth2/v2/auth");
    let param = |name: &str| query(&url, name);
    assert_eq!(param("client_id").as_deref(), Some(super::CLIENT_ID));
    assert_eq!(param("scope").as_deref(), Some(super::SCOPE));
    assert_eq!(param("response_type").as_deref(), Some("code"));
    assert_eq!(param("access_type").as_deref(), Some("offline"));
    assert_eq!(param("prompt").as_deref(), Some("consent"));
    assert_eq!(param("code_challenge_method").as_deref(), Some("S256"));
    assert_eq!(param("state").map(|state| state.len()), Some(43));
    assert_eq!(param("redirect_uri"), Some(callback.redirect_uri.clone()));
    assert_eq!(
        param("code_challenge"),
        Some(code_challenge(&callback.verifier))
    );
    for absent in ["include_granted_scopes", "trigger_onepick", "login_hint"] {
        assert_eq!(param(absent), None, "{absent}");
    }
    assert_eq!(callback.code, "galpi-test-code");
    assert_eq!(callback.picked, Vec::<DriveFileId>::new());
    Ok(())
}

#[tokio::test]
async fn the_picker_adds_onepick_audio_types_and_the_account_hint() -> Result<(), AppError> {
    // Given
    let harness = AuthHarness::start(
        answer("&code=galpi-test-code&picked_file_ids=file-1"),
        no_server(),
    )
    .await?;

    // When
    let callback = harness
        .authorize(&DriveAuthorize {
            picker: true,
            login_hint: Some("user@example.com".to_owned()),
        })
        .await?;

    // Then
    let url = harness.opened_url()?;
    assert_eq!(query(&url, "trigger_onepick").as_deref(), Some("true"));
    assert_eq!(query(&url, "prompt").as_deref(), Some("consent"));
    assert_eq!(
        query(&url, "login_hint").as_deref(),
        Some("user@example.com")
    );
    let mimetypes = query(&url, "mimetypes").unwrap_or_default();
    assert!(mimetypes.contains("audio/x-m4a") && mimetypes.contains("video/mp4"));
    assert_eq!(
        callback
            .picked
            .iter()
            .map(DriveFileId::as_str)
            .collect::<Vec<_>>(),
        ["file-1"]
    );
    Ok(())
}

#[tokio::test]
async fn callbacks_that_do_not_answer_this_request_are_refused() -> Result<(), AppError> {
    let cases: [(Behavior, &str); 5] = [
        (
            Box::new(|_state| "state=someone-else&code=galpi-test-code".to_owned()),
            "DRIVE_STATE_MISMATCH",
        ),
        (answer("&error=access_denied"), "CANCELLED"),
        (answer("&error=server_error"), "DRIVE_CONNECT_FAILED"),
        (answer(""), "DRIVE_CONNECT_FAILED"),
        (
            answer("&code=galpi-test-code&picked_file_ids=a/b"),
            "DRIVE_PICK_INVALID",
        ),
    ];
    for (behavior, code) in cases {
        let harness = AuthHarness::start(behavior, no_server()).await?;
        let error = harness
            .authorize(&CONNECT)
            .await
            .err()
            .ok_or_else(|| AppError::new("TEST", "expected a refusal"))?;
        assert_eq!(error.code, code);
        assert!(!error.message.contains("galpi-test-code"));
    }
    Ok(())
}

#[tokio::test]
async fn the_gateway_exchanges_the_code_on_the_bearers_behalf() -> Result<(), AppError> {
    // Given
    let harness = AuthHarness::start(
        answer(""),
        fixed(200, token_answer(Some("galpi-test-refresh"))),
    )
    .await?;

    // When
    let tokens = harness
        .adapter
        .exchange("galpi-test-gateway", &callback())
        .await
        .map_err(|failure| AppError::new("TEST", format!("{failure:?}")))?;

    // Then: only the tokens are kept
    assert_eq!(
        tokens,
        DriveTokens {
            access_token: "galpi-test-access".to_owned(),
            refresh_token: "galpi-test-refresh".to_owned(),
            expires_at: NOW + 3599,
        }
    );
    let requests = harness.server.requests();
    let exchange = requests
        .first()
        .ok_or_else(|| AppError::new("TEST", "no exchange"))?;
    assert_eq!(exchange.method, "POST");
    assert_eq!(exchange.route(), "/drive/native/token");
    assert_eq!(
        exchange.authorization.as_deref(),
        Some("Bearer galpi-test-gateway")
    );
    assert_eq!(exchange.content_type.as_deref(), Some("application/json"));
    let body = json_body(exchange);
    assert_eq!(body["grant_type"], "authorization_code");
    assert_eq!(body["code"], "galpi-test-code");
    assert_eq!(body["code_verifier"], "galpi-test-verifier");
    assert_eq!(body["redirect_uri"], "http://127.0.0.1:1455/auth/callback");
    assert_eq!(body.as_object().map(serde_json::Map::len), Some(4));
    Ok(())
}

#[tokio::test]
async fn token_failures_map_to_what_the_caller_can_do() -> Result<(), AppError> {
    let cases = [
        (
            400,
            json!({"error": "invalid_grant"}).to_string(),
            DriveTokenFailure::Rejected,
        ),
        (
            400,
            json!({"error": "invalid_request"}).to_string(),
            DriveTokenFailure::Unavailable,
        ),
        (
            401,
            json!({"error": "unauthorized"}).to_string(),
            DriveTokenFailure::AuthRequired,
        ),
        (
            503,
            json!({"error": "drive_unavailable"}).to_string(),
            DriveTokenFailure::Unavailable,
        ),
        (200, "not json".to_owned(), DriveTokenFailure::Unavailable),
        (200, token_answer(None), DriveTokenFailure::Unavailable),
    ];
    for (status, body, expected) in cases {
        let harness = AuthHarness::start(answer(""), fixed(status, body)).await?;
        assert_eq!(
            harness
                .adapter
                .exchange("galpi-test-gateway", &callback())
                .await
                .err(),
            Some(expected),
            "status {status}"
        );
    }

    // And: a gateway that cannot be reached
    let unreachable = DriveOAuthAdapter::with_tuning(
        tuning("http://127.0.0.1:9"),
        Arc::new(FakeBrowser {
            behavior: answer(""),
            urls: Mutex::new(Vec::new()),
        }),
        Arc::new(FixedClock),
    )?;
    assert_eq!(
        unreachable
            .refresh("galpi-test-gateway", "galpi-test-refresh")
            .await
            .err(),
        Some(DriveTokenFailure::Unavailable)
    );
    Ok(())
}

#[tokio::test]
async fn a_renewal_without_a_rotated_refresh_token_keeps_the_old_one() -> Result<(), AppError> {
    // Given
    let harness = AuthHarness::start(answer(""), fixed(200, token_answer(None))).await?;

    // When
    let tokens = harness
        .adapter
        .refresh("galpi-test-gateway", "galpi-test-refresh-old")
        .await
        .map_err(|failure| AppError::new("TEST", format!("{failure:?}")))?;

    // Then
    assert_eq!(tokens.refresh_token, "galpi-test-refresh-old");
    let requests = harness.server.requests();
    let body = requests.first().map_or(Value::Null, json_body);
    assert_eq!(
        body,
        json!({"grant_type": "refresh_token", "refresh_token": "galpi-test-refresh-old"})
    );
    Ok(())
}

#[tokio::test]
async fn revocation_posts_the_token_to_google_and_tolerates_failure() -> Result<(), AppError> {
    // Given
    let harness = AuthHarness::start(answer(""), fixed(500, String::new())).await?;

    // When
    harness.adapter.revoke("galpi-test-refresh").await;

    // Then
    let requests = harness.server.requests();
    let revoke = requests
        .first()
        .ok_or_else(|| AppError::new("TEST", "no revocation"))?;
    assert_eq!(revoke.route(), "/revoke");
    assert_eq!(revoke.field("token").as_deref(), Some("galpi-test-refresh"));
    assert_eq!(revoke.authorization, None);
    Ok(())
}

struct RestHarness {
    server: FakeServer,
    adapter: DriveRestAdapter,
    imports: PathBuf,
}

impl RestHarness {
    async fn start(handler: Handler) -> Result<Self, AppError> {
        let server = FakeServer::start(handler)
            .await
            .map_err(|error| AppError::io("server", &error))?;
        let imports = std::env::temp_dir().join(format!("galpi-drive-imports-{}", Uuid::now_v7()));
        let adapter = DriveRestAdapter::with_tuning(&tuning(&server.base), imports.clone())?;
        Ok(Self {
            server,
            adapter,
            imports,
        })
    }

    fn requests_to(&self, method: &str, route: &str) -> Vec<Recorded> {
        self.server
            .requests()
            .into_iter()
            .filter(|request| request.method == method && request.route() == route)
            .collect()
    }

    fn import_folders(&self) -> usize {
        std::fs::read_dir(&self.imports).map_or(0, Iterator::count)
    }
}

impl Drop for RestHarness {
    fn drop(&mut self) {
        let _removed = std::fs::remove_dir_all(&self.imports);
    }
}

const TOKEN: &str = "galpi-test-drive-access";

fn upload(content: &str) -> LocalUpload {
    LocalUpload {
        name: "meeting_화자별.txt".to_owned(),
        content: content.as_bytes().to_vec(),
        sha256: "galpi-sha".to_owned(),
    }
}

#[tokio::test]
async fn whoami_reads_the_account_email_and_permission_id() -> Result<(), AppError> {
    let harness = RestHarness::start(fixed(
        200,
        json!({"user": {"emailAddress": "user@example.com", "permissionId": "perm-1"}}).to_string(),
    ))
    .await?;

    let account = harness.adapter.whoami(TOKEN).await?;

    assert_eq!(account.email, "user@example.com");
    assert_eq!(account.permission_id, "perm-1");
    let about = harness.requests_to("GET", "/drive/v3/about");
    assert_eq!(
        about
            .first()
            .and_then(|request| request.authorization.clone()),
        Some(format!("Bearer {TOKEN}"))
    );
    Ok(())
}

#[tokio::test]
async fn the_root_folder_is_reused_found_again_or_created() -> Result<(), AppError> {
    // Given: the remembered folder still exists
    let reuse = RestHarness::start(fixed(
        200,
        json!({"id": "root-1", "trashed": false}).to_string(),
    ))
    .await?;
    assert_eq!(
        reuse
            .adapter
            .ensure_root_folder(TOKEN, Some("root-1"))
            .await?,
        "root-1"
    );
    assert!(reuse.requests_to("POST", "/drive/v3/files").is_empty());

    // Given: the remembered folder is gone, but a marked one exists
    let found = RestHarness::start(Arc::new(|request| match request.route() {
        "/drive/v3/files/root-1" => not_found(),
        "/drive/v3/files" => (200, json!({"files": [{"id": "root-2"}]}).to_string()),
        _ => (500, String::new()),
    }))
    .await?;
    assert_eq!(
        found
            .adapter
            .ensure_root_folder(TOKEN, Some("root-1"))
            .await?,
        "root-2"
    );
    let search = found.requests_to("GET", "/drive/v3/files");
    let q = search
        .first()
        .and_then(|request| request.query("q"))
        .unwrap_or_default();
    assert!(
        q.contains("appProperties has { key='galpiRoot' and value='1' }"),
        "{q}"
    );
    assert!(q.contains("trashed=false"), "{q}");

    // Given: nothing is on Drive yet
    let created = RestHarness::start(Arc::new(|request| match request.method.as_str() {
        "GET" => (200, json!({"files": []}).to_string()),
        _ => (200, json!({"id": "root-3"}).to_string()),
    }))
    .await?;
    assert_eq!(
        created.adapter.ensure_root_folder(TOKEN, None).await?,
        "root-3"
    );
    let creation = created.requests_to("POST", "/drive/v3/files");
    let body = creation.first().map_or(Value::Null, json_body);
    assert_eq!(body["name"], "Galpi");
    assert_eq!(body["mimeType"], "application/vnd.google-apps.folder");
    assert_eq!(body["appProperties"]["galpiRoot"], "1");
    Ok(())
}

#[tokio::test]
async fn a_trashed_root_folder_is_replaced() -> Result<(), AppError> {
    let harness = RestHarness::start(Arc::new(|request| match request.route() {
        "/drive/v3/files/root-1" => (200, json!({"id": "root-1", "trashed": true}).to_string()),
        _ if request.method == "GET" => (200, json!({"files": []}).to_string()),
        _ => (200, json!({"id": "root-2"}).to_string()),
    }))
    .await?;

    assert_eq!(
        harness
            .adapter
            .ensure_root_folder(TOKEN, Some("root-1"))
            .await?,
        "root-2"
    );
    Ok(())
}

#[tokio::test]
async fn meeting_folders_are_found_by_key_or_created_under_the_root() -> Result<(), AppError> {
    let key =
        MeetingKey::drive(&DriveFileId::parse("file-1").map_err(|_| AppError::new("TEST", "id"))?);

    // Given: an existing meeting folder
    let found = RestHarness::start(fixed(
        200,
        json!({"files": [{"id": "meeting-1", "webViewLink": "https://drive.google.com/m1"}]})
            .to_string(),
    ))
    .await?;
    let folder = found
        .adapter
        .ensure_meeting_folder(TOKEN, "root-1", &key, "주간 회의")
        .await?;
    assert_eq!(folder.id, "meeting-1");
    assert_eq!(folder.link.as_deref(), Some("https://drive.google.com/m1"));
    let q = found
        .requests_to("GET", "/drive/v3/files")
        .first()
        .and_then(|request| request.query("q"))
        .unwrap_or_default();
    assert!(q.contains("'root-1' in parents"), "{q}");
    assert!(
        q.contains("key='galpiMeeting' and value='drive:file-1'"),
        "{q}"
    );

    // Given: none yet
    let created = RestHarness::start(Arc::new(|request| match request.method.as_str() {
        "GET" => (200, json!({"files": []}).to_string()),
        _ => (
            200,
            json!({"id": "meeting-2", "webViewLink": "https://drive.google.com/m2"}).to_string(),
        ),
    }))
    .await?;
    let folder = created
        .adapter
        .ensure_meeting_folder(TOKEN, "root-1", &key, "주간 회의")
        .await?;
    assert_eq!(folder.id, "meeting-2");
    let body = created
        .requests_to("POST", "/drive/v3/files")
        .first()
        .map_or(Value::Null, json_body);
    assert_eq!(body["name"], "주간 회의");
    assert_eq!(body["parents"], json!(["root-1"]));
    assert_eq!(body["appProperties"]["galpiMeeting"], "drive:file-1");
    Ok(())
}

#[tokio::test]
async fn result_files_are_found_by_kind_with_their_checksums() -> Result<(), AppError> {
    let harness = RestHarness::start(fixed(
        200,
        json!({"files": [{
            "id": "txt-1",
            "sha256Checksum": "abc",
            "appProperties": {"galpiSha256": "def", "galpiKind": "speaker_text"},
            "webViewLink": "https://drive.google.com/t1",
        }]})
        .to_string(),
    ))
    .await?;

    let found = harness
        .adapter
        .find_artifact(TOKEN, "meeting-1", ArtifactKind::SpeakerText)
        .await?
        .ok_or_else(|| AppError::new("TEST", "not found"))?;

    assert_eq!(found.id, "txt-1");
    assert_eq!(found.sha256_checksum.as_deref(), Some("abc"));
    assert_eq!(found.synced_sha256.as_deref(), Some("def"));
    let q = harness
        .requests_to("GET", "/drive/v3/files")
        .first()
        .and_then(|request| request.query("q"))
        .unwrap_or_default();
    assert!(q.contains("'meeting-1' in parents"), "{q}");
    assert!(
        q.contains("key='galpiKind' and value='speaker_text'"),
        "{q}"
    );
    Ok(())
}

#[tokio::test]
async fn new_results_upload_as_one_multipart_request_with_galpi_properties() -> Result<(), AppError>
{
    // Given
    let harness = RestHarness::start(fixed(
        200,
        json!({"id": "txt-1", "webViewLink": "https://drive.google.com/t1", "sha256Checksum": "galpi-sha"})
            .to_string(),
    ))
    .await?;
    let key = MeetingKey::local(Path::new("/tmp/meeting.m4a"));

    // When
    let created = harness
        .adapter
        .create_file(
            TOKEN,
            "meeting-1",
            &key,
            ArtifactKind::SpeakerText,
            &upload("본문"),
        )
        .await?;

    // Then
    assert_eq!(created.id, "txt-1");
    assert_eq!(created.synced_sha256.as_deref(), Some("galpi-sha"));
    let requests = harness.requests_to("POST", "/upload/drive/v3/files");
    let request = requests
        .first()
        .ok_or_else(|| AppError::new("TEST", "no upload"))?;
    assert_eq!(request.query("uploadType").as_deref(), Some("multipart"));
    let content_type = request.content_type.clone().unwrap_or_default();
    let boundary = content_type
        .strip_prefix("multipart/related; boundary=")
        .unwrap_or_default();
    assert!(!boundary.is_empty(), "{content_type}");
    assert!(request.body.contains(&format!("--{boundary}")));
    assert!(
        request
            .body
            .contains("Content-Type: text/plain\r\n\r\n본문")
    );
    let metadata: Value = request
        .body
        .split_once("\r\n\r\n")
        .and_then(|(_, rest)| rest.split_once("\r\n--"))
        .and_then(|(part, _)| serde_json::from_str(part).ok())
        .unwrap_or(Value::Null);
    assert_eq!(metadata["name"], "meeting_화자별.txt");
    assert_eq!(metadata["parents"], json!(["meeting-1"]));
    assert_eq!(metadata["mimeType"], "text/plain");
    assert_eq!(metadata["appProperties"]["galpiMeeting"], key.as_str());
    assert_eq!(metadata["appProperties"]["galpiKind"], "speaker_text");
    assert_eq!(metadata["appProperties"]["galpiSha256"], "galpi-sha");
    Ok(())
}

#[tokio::test]
async fn updates_patch_the_same_file_id() -> Result<(), AppError> {
    let harness = RestHarness::start(fixed(200, json!({"id": "txt-1"}).to_string())).await?;
    let key = MeetingKey::local(Path::new("/tmp/meeting.m4a"));

    let updated = harness
        .adapter
        .update_file(
            TOKEN,
            "txt-1",
            &key,
            ArtifactKind::Minutes,
            &upload("# 회의록"),
        )
        .await?;

    assert_eq!(updated.id, "txt-1");
    let patches = harness.requests_to("PATCH", "/upload/drive/v3/files/txt-1");
    let request = patches
        .first()
        .ok_or_else(|| AppError::new("TEST", "no patch"))?;
    assert!(
        request
            .body
            .contains("Content-Type: text/markdown\r\n\r\n# 회의록")
    );
    assert!(!request.body.contains("\"parents\""));
    Ok(())
}

#[tokio::test]
async fn drive_errors_map_to_stable_codes() -> Result<(), AppError> {
    let quota = json!({"error": {"errors": [{"reason": "storageQuotaExceeded"}]}}).to_string();
    let rate = json!({"error": {"errors": [{"reason": "userRateLimitExceeded"}]}}).to_string();
    let cases = [
        (401, String::new(), "DRIVE_TOKEN_REJECTED"),
        (403, quota, "DRIVE_STORAGE_FULL"),
        (403, rate, "DRIVE_UNAVAILABLE"),
        (403, String::new(), "DRIVE_REQUEST_FAILED"),
        (404, String::new(), "DRIVE_NOT_FOUND"),
        (429, String::new(), "DRIVE_UNAVAILABLE"),
        (500, String::new(), "DRIVE_UNAVAILABLE"),
    ];
    for (status, body, code) in cases {
        let harness = RestHarness::start(fixed(status, body)).await?;
        let error = harness
            .adapter
            .whoami(TOKEN)
            .await
            .err()
            .ok_or_else(|| AppError::new("TEST", "expected a failure"))?;
        assert_eq!(error.code, code, "status {status}");
        assert!(!error.message.contains(TOKEN));
    }
    Ok(())
}

#[tokio::test]
async fn large_results_are_not_uploaded() -> Result<(), AppError> {
    let harness = RestHarness::start(no_server()).await?;
    let directory = std::env::temp_dir().join(format!("galpi-drive-upload-{}", Uuid::now_v7()));
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError::io("dir", &error))?;
    let large = directory.join("large.txt");
    let small = directory.join("small.txt");
    tokio::fs::write(&large, vec![b'a'; 5 * 1024 * 1024 + 1])
        .await
        .map_err(|error| AppError::io("write", &error))?;
    tokio::fs::write(&small, b"abc")
        .await
        .map_err(|error| AppError::io("write", &error))?;

    let refused = harness
        .adapter
        .read_local(&large)
        .await
        .err()
        .map(|error| error.code);
    let read = harness.adapter.read_local(&small).await?;

    assert_eq!(refused.as_deref(), Some("DRIVE_UPLOAD_TOO_LARGE"));
    assert_eq!(read.name, "small.txt");
    assert_eq!(
        read.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let _removed = tokio::fs::remove_dir_all(directory).await;
    Ok(())
}

fn picked(modified: &str) -> PickedFile {
    PickedFile {
        name: "주간 회의.m4a".to_owned(),
        mime_type: "audio/x-m4a".to_owned(),
        can_download: true,
        modified_time: Some(modified.to_owned()),
    }
}

#[tokio::test]
async fn downloads_land_in_a_fresh_folder_with_the_drive_modification_time() -> Result<(), AppError>
{
    // Given
    let harness = RestHarness::start(fixed(200, "galpi-test-audio".to_owned())).await?;
    let id = DriveFileId::parse("file-1").map_err(|_| AppError::new("TEST", "id"))?;
    let (_cancel, mut receiver) = oneshot::channel();

    // When
    let path = harness
        .adapter
        .download(
            TOKEN,
            &id,
            &picked("2026-10-01T09:30:00.000Z"),
            &mut receiver,
        )
        .await?;

    // Then
    assert_eq!(
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .as_deref(),
        Some("주간 회의.m4a")
    );
    assert_eq!(
        tokio::fs::read_to_string(&path)
            .await
            .map_err(|error| AppError::io("read", &error))?,
        "galpi-test-audio"
    );
    let modified = tokio::fs::metadata(&path)
        .await
        .and_then(|metadata| metadata.modified())
        .map_err(|error| AppError::io("metadata", &error))?;
    assert_eq!(
        modified
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .ok(),
        Some(1_790_847_000)
    );
    let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let names: Vec<String> = std::fs::read_dir(&folder)
        .map_err(|error| AppError::io("list", &error))?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["주간 회의.m4a"]);
    let requests = harness.requests_to("GET", "/drive/v3/files/file-1");
    assert_eq!(
        requests
            .first()
            .and_then(|request| request.query("alt"))
            .as_deref(),
        Some("media")
    );
    Ok(())
}

#[tokio::test]
async fn a_cancelled_or_refused_download_leaves_no_folder_behind() -> Result<(), AppError> {
    let id = DriveFileId::parse("file-1").map_err(|_| AppError::new("TEST", "id"))?;

    // Given: the user cancelled before the download began
    let harness = RestHarness::start(fixed(200, "galpi-test-audio".to_owned())).await?;
    let (cancel, mut receiver) = oneshot::channel();
    let _sent = cancel.send(());
    let error = harness
        .adapter
        .download(TOKEN, &id, &picked("2026-10-01T09:30:00Z"), &mut receiver)
        .await
        .err()
        .map(|error| error.code);
    assert_eq!(error.as_deref(), Some("CANCELLED"));
    assert_eq!(harness.import_folders(), 0);

    // Given: Drive refuses the download
    let refused = RestHarness::start(fixed(
        403,
        json!({"error": {"errors": [{"reason": "cannotDownloadFile"}]}}).to_string(),
    ))
    .await?;
    let (_cancel, mut receiver) = oneshot::channel();
    let error = refused
        .adapter
        .download(TOKEN, &id, &picked("2026-10-01T09:30:00Z"), &mut receiver)
        .await
        .err()
        .map(|error| error.code);
    assert_eq!(error.as_deref(), Some("DRIVE_DOWNLOAD_FAILED"));
    assert_eq!(refused.import_folders(), 0);
    Ok(())
}

#[tokio::test]
async fn startup_clears_leftover_imports() -> Result<(), AppError> {
    let harness = RestHarness::start(no_server()).await?;
    std::fs::create_dir_all(harness.imports.join("left-over"))
        .map_err(|error| AppError::io("dir", &error))?;

    harness
        .adapter
        .clear_imports()
        .map_err(|error| AppError::io("clear", &error))?;

    assert!(!harness.imports.exists());
    harness
        .adapter
        .clear_imports()
        .map_err(|error| AppError::io("clear again", &error))?;
    Ok(())
}
