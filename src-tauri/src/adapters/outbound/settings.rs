use super::paths::AppPaths;
use super::platform::Os;
use super::secrets::{Secret, SecretStore};
use crate::application::error::AppError;
use crate::application::ports::SettingsPort;
use crate::domain::engine::{ComputeDevice, EnginePreset};
use crate::domain::roster::{AssistantSettings, GlossaryEntry, Participant};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[derive(Debug)]
pub struct LocalSettingsStore {
    path: PathBuf,
    /// Decides which engines and devices this machine offers.
    os: Os,
    secrets: std::sync::Arc<dyn SecretStore>,
    /// What each secret currently holds, once it has been read.
    ///
    /// Every keychain call is a permission check, and macOS asks the user
    /// about it whenever the app's signature is not one it already trusts.
    /// Caching means one question per secret per launch instead of one per
    /// settings autosave, which fires on every keystroke in the roster.
    cached_secrets: tokio::sync::Mutex<HashMap<Secret, Option<String>>>,
    /// Serializes read-modify-write cycles and caches the parsed file.
    ///
    /// Every save rewrites the whole document, so two concurrent saves would
    /// each read the same starting state and the second would drop the first's
    /// field. Holding the lock across the cycle makes that impossible, and the
    /// cached value spares a read and a parse on the transcription path.
    state: tokio::sync::Mutex<Option<LocalSettings>>,
}

impl LocalSettingsStore {
    /// `secrets` is chosen by the composition root: the Windows Credential
    /// Manager there, the settings file elsewhere (see the secrets module).
    pub fn new(
        app: &AppHandle,
        secrets: std::sync::Arc<dyn SecretStore>,
    ) -> Result<Self, AppError> {
        Ok(Self::build(
            AppPaths::resolve(app)?.root.join("settings.json"),
            Os::current(),
            secrets,
        ))
    }

    fn build(path: PathBuf, os: Os, secrets: std::sync::Arc<dyn SecretStore>) -> Self {
        Self {
            path,
            os,
            secrets,
            cached_secrets: tokio::sync::Mutex::new(HashMap::new()),
            state: tokio::sync::Mutex::new(None),
        }
    }

    #[cfg(test)]
    fn for_path(path: PathBuf) -> Self {
        Self::for_os(path, Os::MacOs)
    }

    #[cfg(test)]
    fn for_os(path: PathBuf, os: Os) -> Self {
        Self::build(
            path,
            os,
            std::sync::Arc::new(super::secrets::InMemorySecrets::default()),
        )
    }

    #[cfg(test)]
    fn with_secrets(path: PathBuf, secrets: std::sync::Arc<dyn SecretStore>) -> Self {
        Self::build(path, Os::MacOs, secrets)
    }

    /// Read a secret, moving it out of the settings file the first time.
    ///
    /// Installs that predate keychain storage still have their tokens in
    /// `settings.json`. Reading one migrates it and clears the plaintext copy,
    /// so the file stops carrying the secret without the user re-entering it.
    async fn secret(&self, secret: Secret) -> Result<Option<String>, AppError> {
        if let Some(cached) = self.cached_secrets.lock().await.get(&secret) {
            return Ok(cached.clone());
        }
        let stored = self.secrets.read(secret)?;
        if stored.is_some() {
            let _previous = self
                .cached_secrets
                .lock()
                .await
                .insert(secret, stored.clone());
            self.note_secret_present(secret, true).await?;
            return Ok(stored);
        }
        let legacy = {
            let state = self.load().await?;
            state.as_ref().and_then(|settings| match secret {
                Secret::HuggingFaceToken => settings.hugging_face_token.clone(),
                Secret::AssistantApiKey => settings.assistant_api_key.clone(),
            })
        };
        let Some(value) = legacy else {
            let _previous = self.cached_secrets.lock().await.insert(secret, None);
            return Ok(None);
        };
        self.store_secret(secret, Some(&value)).await?;
        Ok(Some(value))
    }

    /// Publish a secret to the keychain and keep the file free of plaintext.
    ///
    /// A value identical to the one already stored does nothing at all. The
    /// settings sheet autosaves the whole document whenever any field changes,
    /// so without this an edit to a participant's name would rewrite the API
    /// key and ask the user to authorize the keychain again.
    async fn store_secret(&self, secret: Secret, value: Option<&str>) -> Result<(), AppError> {
        let unchanged = {
            let cache = self.cached_secrets.lock().await;
            cache.get(&secret) == Some(&value.map(str::to_owned))
        };
        if unchanged {
            return Ok(());
        }
        self.secrets.write(secret, value)?;
        let _previous = self
            .cached_secrets
            .lock()
            .await
            .insert(secret, value.map(str::to_owned));
        let present = value.is_some();
        // A store that holds the secret itself wants the file scrubbed; the
        // settings file is the store, so the value stays where it is.
        let kept_here = self.secrets.keeps_plaintext_in_settings();
        let retained = kept_here.then(|| value.map(str::to_owned));
        self.update(|settings| match secret {
            Secret::HuggingFaceToken => {
                if let Some(value) = retained {
                    settings.hugging_face_token = value;
                } else {
                    settings.hugging_face_token = None;
                }
                settings.hugging_face_token_stored = present;
            }
            Secret::AssistantApiKey => {
                if let Some(value) = retained {
                    settings.assistant_api_key = value;
                } else {
                    settings.assistant_api_key = None;
                }
                settings.assistant_api_key_stored = present;
            }
        })
        .await
    }

    /// Record that a secret exists, so the flag survives into the next launch.
    async fn note_secret_present(&self, secret: Secret, present: bool) -> Result<(), AppError> {
        let already = {
            let state = self.load().await?;
            state.as_ref().is_some_and(|settings| match secret {
                Secret::HuggingFaceToken => settings.hugging_face_token_stored == present,
                Secret::AssistantApiKey => settings.assistant_api_key_stored == present,
            })
        };
        if already {
            return Ok(());
        }
        self.update(|settings| match secret {
            Secret::HuggingFaceToken => settings.hugging_face_token_stored = present,
            Secret::AssistantApiKey => settings.assistant_api_key_stored = present,
        })
        .await
    }

    /// Whether a secret is on file.
    ///
    /// Answered from the settings file, which is what keeps opening the sheet
    /// off the keychain. The one exception is an install whose secret was moved
    /// into the keychain before this flag existed: that is read once, and the
    /// flag it writes means it is never read again.
    async fn secret_stored(&self, secret: Secret) -> Result<bool, AppError> {
        if let Some(cached) = self.cached_secrets.lock().await.get(&secret) {
            return Ok(cached.is_some());
        }
        let recorded = {
            let state = self.load().await?;
            state.as_ref().map(|settings| match secret {
                Secret::HuggingFaceToken => (
                    settings.hugging_face_token_stored,
                    settings.hugging_face_token.is_some(),
                ),
                Secret::AssistantApiKey => (
                    settings.assistant_api_key_stored,
                    settings.assistant_api_key.is_some(),
                ),
            })
        };
        match recorded {
            Some((true, _) | (_, true)) => Ok(true),
            _ => Ok(self.secret(secret).await?.is_some()),
        }
    }

    /// Read the settings, parsing the file only the first time.
    async fn load(&self) -> Result<tokio::sync::MutexGuard<'_, Option<LocalSettings>>, AppError> {
        let mut state = self.state.lock().await;
        if state.is_none() {
            *state = Some(read_settings(&self.path).await?);
        }
        Ok(state)
    }

    /// Apply a change and publish it, keeping the cache and file in step.
    async fn update(&self, change: impl FnOnce(&mut LocalSettings)) -> Result<(), AppError> {
        let mut state = self.load().await?;
        let settings = state
            .as_mut()
            .ok_or_else(|| AppError::new("SETTINGS_INVALID", "앱 설정을 읽지 못했습니다."))?;
        change(settings);
        let result = store_settings(&self.path, settings).await;
        if result.is_err() {
            // The file no longer matches the cache, so drop it and re-read.
            *state = None;
        }
        result
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
struct LocalSettings {
    // Unset means "this platform's default". Both are skipped when `None`
    // because a serialized `null` makes older builds fail to parse the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_preset: Option<EnginePreset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compute_device: Option<ComputeDevice>,
    hugging_face_token: Option<String>,
    assistant_api_key: Option<String>,
    hugging_face_token_stored: bool,
    assistant_api_key_stored: bool,
    assistant_model: Option<String>,
    assistant_base_url: Option<String>,
    assistant_reasoning_effort: Option<String>,
    assistant_background: Option<String>,
    participants: Vec<Participant>,
    glossary: Vec<GlossaryEntry>,
}

impl LocalSettings {
    fn is_empty(&self) -> bool {
        self.engine_preset.is_none()
            && self.compute_device.is_none()
            && self.hugging_face_token.is_none()
            && self.assistant_api_key.is_none()
            && !self.hugging_face_token_stored
            && !self.assistant_api_key_stored
            && self.assistant_model.is_none()
            && self.assistant_base_url.is_none()
            && self.assistant_reasoning_effort.is_none()
            && self.assistant_background.is_none()
            && self.participants.is_empty()
            && self.glossary.is_empty()
    }
}

#[async_trait]
impl SettingsPort for LocalSettingsStore {
    async fn hugging_face_token_stored(&self) -> Result<bool, AppError> {
        self.secret_stored(Secret::HuggingFaceToken).await
    }

    async fn load_hugging_face_token(&self) -> Result<Option<String>, AppError> {
        self.secret(Secret::HuggingFaceToken).await
    }

    async fn save_hugging_face_token(&self, token: Option<String>) -> Result<(), AppError> {
        self.store_secret(Secret::HuggingFaceToken, token.as_deref())
            .await
    }

    async fn load_engine_preset(&self) -> Result<EnginePreset, AppError> {
        let stored = self
            .load()
            .await?
            .as_ref()
            .and_then(|settings| settings.engine_preset);
        // A stored preset this machine cannot run (a Qwen3 choice synced from
        // a Mac, say) falls back to the platform default.
        Ok(stored
            .filter(|preset| self.os.presets().contains(preset))
            .or_else(|| self.os.presets().first().copied())
            .unwrap_or_default())
    }

    async fn save_engine_preset(&self, preset: EnginePreset) -> Result<(), AppError> {
        if !self.os.presets().contains(&preset) {
            return Err(AppError::new(
                "ENGINE_PRESET_UNAVAILABLE",
                "이 컴퓨터에서는 사용할 수 없는 전사 엔진입니다.",
            ));
        }
        self.update(|settings| settings.engine_preset = Some(preset))
            .await
    }

    async fn load_compute_device(&self) -> Result<ComputeDevice, AppError> {
        let stored = self
            .load()
            .await?
            .as_ref()
            .and_then(|settings| settings.compute_device);
        Ok(stored
            .filter(|device| self.os.devices().contains(device))
            .unwrap_or_default())
    }

    async fn save_compute_device(&self, device: ComputeDevice) -> Result<(), AppError> {
        if !self.os.devices().contains(&device) {
            return Err(AppError::new(
                "COMPUTE_DEVICE_UNAVAILABLE",
                "이 컴퓨터에서는 선택할 수 없는 연산 장치입니다.",
            ));
        }
        self.update(|settings| settings.compute_device = Some(device))
            .await
    }

    async fn load_assistant_api_key(&self) -> Result<Option<String>, AppError> {
        self.secret(Secret::AssistantApiKey).await
    }

    async fn load_assistant(&self) -> Result<AssistantSettings, AppError> {
        let api_key_stored = self.secret_stored(Secret::AssistantApiKey).await?;
        let state = self.load().await?;
        let settings = state
            .as_ref()
            .ok_or_else(|| AppError::new("SETTINGS_INVALID", "앱 설정을 읽지 못했습니다."))?;
        Ok(AssistantSettings {
            api_key_stored,
            model: settings.assistant_model.clone(),
            base_url: settings.assistant_base_url.clone(),
            reasoning_effort: settings.assistant_reasoning_effort.clone(),
            background: settings.assistant_background.clone(),
            participants: settings.participants.clone(),
            glossary: settings.glossary.clone(),
        })
    }

    async fn save_assistant_api_key(&self, key: Option<String>) -> Result<(), AppError> {
        self.store_secret(Secret::AssistantApiKey, key.as_deref())
            .await
    }

    /// Save everything except the key, which has its own entry point.
    ///
    /// The sheet autosaves this whole document whenever any field changes, and
    /// it does not hold the key — it only knows one exists. Writing the secret
    /// from here would erase it on the first keystroke.
    async fn save_assistant(&self, assistant: AssistantSettings) -> Result<(), AppError> {
        self.update(|settings| {
            settings.assistant_model = assistant.model;
            settings.assistant_base_url = assistant.base_url;
            settings.assistant_reasoning_effort = assistant.reasoning_effort;
            settings.assistant_background = assistant.background;
            settings.participants = assistant.participants;
            settings.glossary = assistant.glossary;
        })
        .await
    }
}

async fn read_settings(path: &Path) -> Result<LocalSettings, AppError> {
    match tokio::fs::read_to_string(path).await {
        Ok(contents) => serde_json::from_str(&contents).map_err(|error| {
            AppError::new(
                "SETTINGS_INVALID",
                format!("앱 설정 파일을 읽지 못했습니다: {error}"),
            )
        }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(LocalSettings::default()),
        Err(error) => Err(AppError::io("앱 설정 파일을 읽지 못했습니다", &error)),
    }
}

async fn store_settings(path: &Path, settings: &LocalSettings) -> Result<(), AppError> {
    if settings.is_empty() {
        return remove_settings(path).await;
    }
    write_settings(path, settings).await
}

async fn write_settings(path: &Path, settings: &LocalSettings) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("SETTINGS_PATH_ERROR", "앱 설정 경로가 올바르지 않습니다."))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| AppError::io("앱 설정 폴더를 만들지 못했습니다", &error))?;
    let contents = serde_json::to_vec(settings).map_err(|error| {
        AppError::new(
            "SETTINGS_SERIALIZE_ERROR",
            format!("앱 설정을 저장 형식으로 바꾸지 못했습니다: {error}"),
        )
    })?;
    let temporary = path.with_extension("json.part");
    tokio::fs::write(&temporary, contents)
        .await
        .map_err(|error| AppError::io("임시 앱 설정 파일을 쓰지 못했습니다", &error))?;
    // Windows keeps the ACL inherited from the per-user app data folder.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|error| AppError::io("앱 설정 파일 권한을 지정하지 못했습니다", &error))?;
    }
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        let _cleanup_result = tokio::fs::remove_file(&temporary).await;
        return Err(AppError::io("앱 설정 파일을 교체하지 못했습니다", &error));
    }
    Ok(())
}

async fn remove_settings(path: &Path) -> Result<(), AppError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::io("저장된 앱 설정을 지우지 못했습니다", &error)),
    }
}

#[cfg(test)]
mod tests {
    use super::super::platform::Os;
    use super::super::secrets::{InMemorySecrets, Secret, SecretStore};
    use super::LocalSettingsStore;
    use crate::application::error::AppError;
    use crate::application::ports::SettingsPort;
    use crate::domain::engine::{ComputeDevice, EnginePreset};
    use crate::domain::roster::{AssistantSettings, GlossaryEntry, Participant};
    use uuid::Uuid;

    #[tokio::test]
    async fn a_corrupted_settings_file_is_reported_rather_than_reset()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: a settings file that is not valid JSON
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        tokio::fs::create_dir_all(&directory).await?;
        let path = directory.join("settings.json");
        tokio::fs::write(&path, "{ not json").await?;
        let store = LocalSettingsStore::for_path(path.clone());

        // When
        let result = store.load_assistant().await;

        // Then: the caller learns the file is bad, and it is still on disk
        assert_eq!(
            result.err().map(|error| error.code),
            Some("SETTINGS_INVALID".to_owned())
        );
        assert!(tokio::fs::try_exists(&path).await?);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn keeps_the_token_out_of_the_settings_file() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        let path = directory.join("settings.json");
        let store = LocalSettingsStore::for_path(path.clone());

        // When: a token is saved alongside an ordinary preference
        store
            .save_hugging_face_token(Some("hf_saved".to_owned()))
            .await?;
        store.save_engine_preset(EnginePreset::WhisperX).await?;

        // Then: it reads back, but the file on disk never carries it
        assert_eq!(
            store.load_hugging_face_token().await?.as_deref(),
            Some("hf_saved")
        );
        let document = tokio::fs::read_to_string(&path).await?;
        assert!(
            !document.contains("hf_saved"),
            "the token must not be written in plaintext:\n{document}"
        );
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &tokio::fs::metadata(&path).await?.permissions()
            ) & 0o777,
            0o600
        );

        store.save_hugging_face_token(None).await?;
        assert_eq!(store.load_hugging_face_token().await?, None);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn an_unchanged_secret_never_reaches_the_keychain_again()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: a stored key, already read once
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        let secrets = std::sync::Arc::new(InMemorySecrets::default());
        let store =
            LocalSettingsStore::with_secrets(directory.join("settings.json"), secrets.clone());
        let assistant = |glossary: Vec<GlossaryEntry>| AssistantSettings {
            api_key_stored: true,
            model: Some("glm-5.3".to_owned()),
            base_url: None,
            reasoning_effort: None,
            background: None,
            participants: Vec::new(),
            glossary,
        };
        store
            .save_assistant_api_key(Some("zai_key".to_owned()))
            .await?;
        store.save_assistant(assistant(Vec::new())).await?;
        let after_first_save = secrets.writes();

        // When: the sheet autosaves again because an unrelated field changed
        store
            .save_assistant(assistant(vec![GlossaryEntry {
                id: "g1".to_owned(),
                term: "갈피".to_owned(),
                description: None,
            }]))
            .await?;
        store.save_assistant(assistant(Vec::new())).await?;

        // Then: the keychain was never asked again, and the key is intact
        assert_eq!(secrets.writes(), after_first_save);
        assert_eq!(
            store.load_assistant_api_key().await?.as_deref(),
            Some("zai_key")
        );
        // Loading the settings reports that a key exists without reading it.
        let loaded = store.load_assistant().await?;
        assert!(loaded.api_key_stored);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn a_secret_is_read_from_the_keychain_once_per_launch()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        let secrets = std::sync::Arc::new(InMemorySecrets::default());
        let store =
            LocalSettingsStore::with_secrets(directory.join("settings.json"), secrets.clone());
        store
            .save_hugging_face_token(Some("hf_saved".to_owned()))
            .await?;
        let reads_before = secrets.reads();

        // When: several parts of the app ask for the token
        for _ in 0..5 {
            assert_eq!(
                store.load_hugging_face_token().await?.as_deref(),
                Some("hf_saved")
            );
        }

        // Then: the keychain answered once and the cache answered the rest
        assert_eq!(secrets.reads(), reads_before);
        // Storing only in the keychain leaves nothing worth keeping on disk, so
        // the settings file may never have been created.
        let _removed = tokio::fs::remove_dir_all(directory).await;
        Ok(())
    }

    #[tokio::test]
    async fn a_secret_moved_before_the_flag_existed_is_found_once_and_remembered()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: the keychain holds a token, but the settings file predates the
        // flag that records it — the state an install left in by the previous
        // release, where the plaintext was cleared and nothing took its place
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        tokio::fs::create_dir_all(&directory).await?;
        let path = directory.join("settings.json");
        tokio::fs::write(&path, r#"{"enginePreset":"whisperx"}"#).await?;
        let secrets = std::sync::Arc::new(InMemorySecrets::default());
        secrets.write(Secret::HuggingFaceToken, Some("hf_migrated"))?;
        let store = LocalSettingsStore::with_secrets(path.clone(), secrets.clone());

        // When: the sheet asks whether a token is saved
        assert!(store.hugging_face_token_stored().await?);
        let reads_after_first = secrets.reads();

        // Then: a fresh process reads the file, not the keychain
        let next_launch = LocalSettingsStore::with_secrets(path, secrets.clone());
        assert!(next_launch.hugging_face_token_stored().await?);
        assert_eq!(secrets.reads(), reads_after_first);

        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn the_settings_file_store_keeps_the_value_it_is_given()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: the store the app actually ships with until it is signed
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        let path = directory.join("settings.json");
        let store = LocalSettingsStore::with_secrets(
            path.clone(),
            std::sync::Arc::new(super::super::secrets::SettingsFile),
        );

        // When
        store
            .save_hugging_face_token(Some("hf_saved".to_owned()))
            .await?;

        // Then: the token is readable, reported as stored, and in the document,
        // because with this store the document is where it lives
        assert_eq!(
            store.load_hugging_face_token().await?.as_deref(),
            Some("hf_saved")
        );
        assert!(store.hugging_face_token_stored().await?);
        assert!(tokio::fs::read_to_string(&path).await?.contains("hf_saved"));
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &tokio::fs::metadata(&path).await?.permissions()
            ) & 0o777,
            0o600
        );

        // And clearing it removes the value rather than leaving it behind
        store.save_hugging_face_token(None).await?;
        assert!(!store.hugging_face_token_stored().await?);
        let document = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        assert!(!document.contains("hf_saved"), "cleared token survived");

        let _removed = tokio::fs::remove_dir_all(directory).await;
        Ok(())
    }

    #[tokio::test]
    async fn migrates_a_token_left_in_an_older_settings_file()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: a settings file written before tokens moved to the keychain
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        tokio::fs::create_dir_all(&directory).await?;
        let path = directory.join("settings.json");
        tokio::fs::write(&path, r#"{"huggingFaceToken":"hf_legacy"}"#).await?;
        let store = LocalSettingsStore::for_path(path.clone());

        // When: the token is read for the first time
        let token = store.load_hugging_face_token().await?;

        // Then: the user keeps their token and the plaintext copy is gone
        assert_eq!(token.as_deref(), Some("hf_legacy"));
        assert_eq!(
            store.load_hugging_face_token().await?.as_deref(),
            Some("hf_legacy")
        );
        let document = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        assert!(
            !document.contains("hf_legacy"),
            "plaintext survived:\n{document}"
        );
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn keeps_assistant_settings_when_the_hugging_face_token_is_cleared()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        let store = LocalSettingsStore::for_path(directory.join("settings.json"));
        store
            .save_hugging_face_token(Some("hf_saved".to_owned()))
            .await?;
        store
            .save_assistant_api_key(Some("zai_key".to_owned()))
            .await?;
        store
            .save_assistant(AssistantSettings {
                api_key_stored: true,
                model: Some("glm-5.2".to_owned()),
                base_url: Some("https://openrouter.ai/api/v1".to_owned()),
                reasoning_effort: Some("max".to_owned()),
                background: Some("제품: 갈피".to_owned()),
                participants: vec![Participant {
                    id: "hb".to_owned(),
                    name: "하빈".to_owned(),
                    team: Some("갈피팀".to_owned()),
                    role: Some("팀리더".to_owned()),
                    description: Some("녹음 파이프라인 담당".to_owned()),
                    aliases: vec!["프로님".to_owned()],
                }],
                glossary: vec![GlossaryEntry {
                    id: "term-galpi".to_owned(),
                    term: "갈피".to_owned(),
                    description: Some("회의 녹음·전사 데스크톱 앱".to_owned()),
                }],
            })
            .await?;

        // When
        store.save_hugging_face_token(None).await?;

        // Then
        let assistant = store.load_assistant().await?;
        assert!(assistant.api_key_stored);
        assert_eq!(
            store.load_assistant_api_key().await?.as_deref(),
            Some("zai_key")
        );
        assert_eq!(assistant.model.as_deref(), Some("glm-5.2"));
        assert_eq!(
            assistant.base_url.as_deref(),
            Some("https://openrouter.ai/api/v1")
        );
        assert_eq!(assistant.reasoning_effort.as_deref(), Some("max"));
        assert_eq!(assistant.background.as_deref(), Some("제품: 갈피"));
        let saved = assistant
            .participants
            .first()
            .ok_or_else(|| AppError::new("TEST_ERROR", "participant was not persisted"))?;
        assert_eq!(saved.name, "하빈");
        assert_eq!(saved.aliases, ["프로님"]);
        let term = assistant
            .glossary
            .first()
            .ok_or_else(|| AppError::new("TEST_ERROR", "glossary entry was not persisted"))?;
        assert_eq!(term.term, "갈피");
        assert_eq!(
            term.description.as_deref(),
            Some("회의 녹음·전사 데스크톱 앱")
        );
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }
    /// The store the app actually ships with, which keeps the key in the file.
    fn shipping_store(path: std::path::PathBuf) -> LocalSettingsStore {
        LocalSettingsStore::with_secrets(
            path,
            std::sync::Arc::new(super::super::secrets::SettingsFile),
        )
    }

    #[tokio::test]
    async fn the_assistant_key_survives_a_relaunch() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a saved key and the settings written beside it
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        let path = directory.join("settings.json");
        let store = shipping_store(path.clone());
        store
            .save_assistant_api_key(Some("zai_key".to_owned()))
            .await?;
        store
            .save_assistant(AssistantSettings {
                api_key_stored: true,
                model: Some("glm-5.3-flash".to_owned()),
                ..AssistantSettings::default()
            })
            .await?;

        // When: the app is launched again
        let next_launch = shipping_store(path.clone());

        // Then: the refinement still finds the key, and the sheet still says so
        assert_eq!(
            next_launch.load_assistant_api_key().await?.as_deref(),
            Some("zai_key")
        );
        let loaded = next_launch.load_assistant().await?;
        assert!(loaded.api_key_stored);
        assert_eq!(loaded.model.as_deref(), Some("glm-5.3-flash"));
        let _removed = tokio::fs::remove_dir_all(directory).await;
        Ok(())
    }

    #[tokio::test]
    async fn an_autosave_cannot_erase_the_stored_assistant_key()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: a stored key, and a settings sheet that never received it
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        let path = directory.join("settings.json");
        let store = shipping_store(path.clone());
        store
            .save_assistant_api_key(Some("zai_key".to_owned()))
            .await?;

        // When: the sheet autosaves because an unrelated field changed
        store
            .save_assistant(AssistantSettings {
                api_key_stored: true,
                model: Some("glm-5-turbo".to_owned()),
                glossary: vec![GlossaryEntry {
                    id: "term-1".to_owned(),
                    term: "갈피".to_owned(),
                    description: None,
                }],
                ..AssistantSettings::default()
            })
            .await?;

        // Then: the key is untouched, here and on the next launch
        assert_eq!(
            store.load_assistant_api_key().await?.as_deref(),
            Some("zai_key")
        );
        assert!(store.load_assistant().await?.api_key_stored);
        assert_eq!(
            shipping_store(path)
                .load_assistant_api_key()
                .await?
                .as_deref(),
            Some("zai_key")
        );
        let _removed = tokio::fs::remove_dir_all(directory).await;
        Ok(())
    }

    #[tokio::test]
    async fn clearing_the_assistant_key_removes_it_from_the_document()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let directory = std::env::temp_dir().join(format!("galpi-settings-{}", Uuid::now_v7()));
        let path = directory.join("settings.json");
        let store = shipping_store(path.clone());
        store
            .save_assistant_api_key(Some("zai_key".to_owned()))
            .await?;
        assert!(tokio::fs::read_to_string(&path).await?.contains("zai_key"));

        // When
        store.save_assistant_api_key(None).await?;

        // Then
        assert_eq!(store.load_assistant_api_key().await?, None);
        assert!(!store.load_assistant().await?.api_key_stored);
        let document = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        assert!(!document.contains("zai_key"), "cleared key survived");
        let _removed = tokio::fs::remove_dir_all(directory).await;
        Ok(())
    }

    fn temp_settings(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("galpi-{label}-{}", Uuid::now_v7()))
    }

    #[tokio::test]
    async fn os_windows_defaults_to_whisperx_and_cpu() -> Result<(), Box<dyn std::error::Error>> {
        // Given: Windows rules and nothing saved
        let directory = temp_settings("win-default");
        let store = LocalSettingsStore::for_os(directory.join("settings.json"), Os::Windows);

        // When / Then
        assert_eq!(store.load_engine_preset().await?, EnginePreset::WhisperX);
        assert_eq!(store.load_compute_device().await?, ComputeDevice::Cpu);
        Ok(())
    }

    #[tokio::test]
    async fn os_windows_refuses_to_save_qwen3() -> Result<(), Box<dyn std::error::Error>> {
        // Given
        let directory = temp_settings("win-qwen3");
        let store = LocalSettingsStore::for_os(directory.join("settings.json"), Os::Windows);

        // When
        let result = store.save_engine_preset(EnginePreset::Qwen3).await;

        // Then
        assert_eq!(
            result.err().map(|error| error.code),
            Some("ENGINE_PRESET_UNAVAILABLE".to_owned())
        );
        assert_eq!(store.load_engine_preset().await?, EnginePreset::WhisperX);
        Ok(())
    }

    #[tokio::test]
    async fn os_windows_corrects_a_stored_qwen3_preset() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a file written on a Mac
        let directory = temp_settings("win-corrects");
        tokio::fs::create_dir_all(&directory).await?;
        let path = directory.join("settings.json");
        tokio::fs::write(&path, r#"{"enginePreset":"qwen3"}"#).await?;
        let store = LocalSettingsStore::for_os(path, Os::Windows);

        // When / Then
        assert_eq!(store.load_engine_preset().await?, EnginePreset::WhisperX);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn os_windows_saves_and_reloads_the_cuda_device() -> Result<(), Box<dyn std::error::Error>>
    {
        // Given
        let directory = temp_settings("win-cuda");
        let path = directory.join("settings.json");
        let store = LocalSettingsStore::for_os(path.clone(), Os::Windows);

        // When
        store.save_compute_device(ComputeDevice::Cuda).await?;

        // Then: the same process and a relaunch both see it
        assert_eq!(store.load_compute_device().await?, ComputeDevice::Cuda);
        let relaunched = LocalSettingsStore::for_os(path, Os::Windows);
        assert_eq!(relaunched.load_compute_device().await?, ComputeDevice::Cuda);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn os_macos_keeps_qwen3_and_has_no_device_choice()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: an older settings file without the new keys
        let directory = temp_settings("mac-compat");
        tokio::fs::create_dir_all(&directory).await?;
        let path = directory.join("settings.json");
        tokio::fs::write(&path, r#"{"assistantModel":"glm"}"#).await?;
        let store = LocalSettingsStore::for_os(path, Os::MacOs);

        // When / Then
        assert_eq!(store.load_engine_preset().await?, EnginePreset::Qwen3);
        assert_eq!(store.load_compute_device().await?, ComputeDevice::Cpu);
        assert_eq!(
            store
                .save_compute_device(ComputeDevice::Cuda)
                .await
                .err()
                .map(|error| error.code),
            Some("COMPUTE_DEVICE_UNAVAILABLE".to_owned())
        );
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn unset_engine_keys_are_never_written_so_older_builds_can_still_read_the_file()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: other settings saved while preset and device were never chosen
        let directory = temp_settings("rollback");
        let path = directory.join("settings.json");
        let store = LocalSettingsStore::for_os(path.clone(), Os::MacOs);
        store
            .save_assistant(AssistantSettings {
                model: Some("glm-5.3".to_owned()),
                ..AssistantSettings::default()
            })
            .await?;

        // Then: neither key is present, not even as null
        let document = tokio::fs::read_to_string(&path).await?;
        assert!(!document.contains("enginePreset"), "{document}");
        assert!(!document.contains("computeDevice"), "{document}");

        // When: the preset is chosen explicitly
        store.save_engine_preset(EnginePreset::WhisperX).await?;

        // Then: it is persisted
        let document = tokio::fs::read_to_string(&path).await?;
        assert!(
            document.contains(r#""enginePreset":"whisperx""#),
            "{document}"
        );
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }
}
