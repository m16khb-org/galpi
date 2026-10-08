//! ChatGPT account persistence on top of the shared settings document.
//!
//! Everything the sheet shows lives in plain flags of `settings.json`, so
//! opening it never reads a secret. Only the token record is a secret, and it
//! rides the same `SecretStore` path as the API key: written when it changes,
//! never when it is merely saved again.

use super::super::secrets::Secret;
use super::{LocalSettings, LocalSettingsStore};
use crate::application::error::AppError;
use crate::application::ports::ChatGptStore;
use crate::domain::chatgpt::{
    AgentHostId, AssistantAuthMode, ChatGptAccountState, ChatGptAccountView, ChatGptPreferences,
    ChatGptRegistration, ChatGptSettings, ChatGptTokens,
};
use async_trait::async_trait;

impl LocalSettingsStore {
    /// Read a value out of the settings document without touching any secret.
    async fn snapshot<T>(&self, read: impl FnOnce(&LocalSettings) -> T) -> Result<T, AppError> {
        let state = self.load().await?;
        state
            .as_ref()
            .map(read)
            .ok_or_else(|| AppError::new("SETTINGS_INVALID", "앱 설정을 읽지 못했습니다."))
    }

    async fn store_tokens(&self, tokens: &ChatGptTokens) -> Result<(), AppError> {
        let record = serde_json::to_string(tokens).map_err(|error| {
            AppError::new(
                "SETTINGS_SERIALIZE_ERROR",
                format!("ChatGPT 로그인 정보를 저장 형식으로 바꾸지 못했습니다: {error}"),
            )
        })?;
        self.store_secret(Secret::ChatGptTokens, Some(&record))
            .await
    }
}

fn account_view(settings: &LocalSettings) -> ChatGptAccountView {
    let registered = settings.chatgpt_client_id.is_some();
    let tokens_stored = settings.chatgpt_tokens_stored || settings.chatgpt_tokens.is_some();
    let state = match (registered, tokens_stored) {
        (false, _) => ChatGptAccountState::SignedOut,
        (true, false) => ChatGptAccountState::SignInRequired,
        (true, true) => ChatGptAccountState::SignedIn,
    };
    ChatGptAccountView {
        state,
        email: registered.then(|| settings.chatgpt_email.clone()).flatten(),
    }
}

fn registration(settings: &LocalSettings) -> Option<ChatGptRegistration> {
    ChatGptRegistration::new(
        settings.chatgpt_client_id.clone()?,
        settings.chatgpt_subject.clone()?,
        settings.chatgpt_email.clone(),
        settings.chatgpt_needs_consent,
    )
    .ok()
}

#[async_trait]
impl ChatGptStore for LocalSettingsStore {
    async fn load_settings(&self) -> Result<ChatGptSettings, AppError> {
        self.snapshot(|settings| ChatGptSettings {
            auth_mode: settings.chatgpt_auth_mode,
            model: settings.chatgpt_model.clone(),
            welcome_acknowledged: settings.chatgpt_welcome_acknowledged,
            account: account_view(settings),
        })
        .await
    }

    async fn save_preferences(&self, preferences: ChatGptPreferences) -> Result<(), AppError> {
        self.update(|settings| {
            settings.chatgpt_auth_mode = preferences.auth_mode;
            settings.chatgpt_model = preferences.model;
            settings.chatgpt_welcome_acknowledged = preferences.welcome_acknowledged;
        })
        .await
    }

    async fn load_or_create_host_id(&self) -> Result<AgentHostId, AppError> {
        let existing = self
            .snapshot(|settings| {
                settings
                    .chatgpt_host_id
                    .as_deref()
                    .and_then(AgentHostId::parse)
            })
            .await?;
        if let Some(id) = existing {
            return Ok(id);
        }
        // Decided under the document lock, so two callers cannot mint two ids.
        let mut minted = None;
        self.update(|settings| {
            let id = settings
                .chatgpt_host_id
                .as_deref()
                .and_then(AgentHostId::parse)
                .unwrap_or_else(AgentHostId::generate);
            settings.chatgpt_host_id = Some(id.as_str().to_owned());
            minted = Some(id);
        })
        .await?;
        minted.ok_or_else(|| {
            AppError::new(
                "SETTINGS_INVALID",
                "에이전트 호스트 ID를 만들지 못했습니다.",
            )
        })
    }

    async fn load_registration(&self) -> Result<Option<ChatGptRegistration>, AppError> {
        self.snapshot(registration).await
    }

    /// A record that no longer parses is as good as none: signing in again
    /// replaces it, and refusing to start would strand the account.
    async fn load_tokens(&self) -> Result<Option<ChatGptTokens>, AppError> {
        Ok(self
            .secret(Secret::ChatGptTokens)
            .await?
            .and_then(|record| serde_json::from_str(&record).ok()))
    }

    /// The registration goes first: if the secret write then fails, the account
    /// reads as "sign in again" rather than as tokens with no client.
    async fn save_session(
        &self,
        registration: ChatGptRegistration,
        tokens: Option<ChatGptTokens>,
    ) -> Result<(), AppError> {
        self.update(|settings| {
            settings.chatgpt_client_id = Some(registration.client_id().to_owned());
            settings.chatgpt_subject = Some(registration.subject().to_owned());
            settings.chatgpt_email = registration.email().map(str::to_owned);
            settings.chatgpt_needs_consent = registration.needs_consent();
        })
        .await?;
        match tokens {
            Some(tokens) => self.store_tokens(&tokens).await,
            None => Ok(()),
        }
    }

    async fn replace_tokens(&self, tokens: ChatGptTokens) -> Result<(), AppError> {
        self.store_tokens(&tokens).await
    }

    async fn clear_tokens(&self) -> Result<(), AppError> {
        self.store_secret(Secret::ChatGptTokens, None).await
    }

    /// Tokens first, so an interruption leaves "sign in again" rather than
    /// tokens with no registration. The host id stays: it names this install.
    async fn clear_session(&self) -> Result<(), AppError> {
        self.store_secret(Secret::ChatGptTokens, None).await?;
        self.update(|settings| {
            settings.chatgpt_client_id = None;
            settings.chatgpt_subject = None;
            settings.chatgpt_email = None;
            settings.chatgpt_needs_consent = false;
            settings.chatgpt_model = None;
            settings.chatgpt_auth_mode = AssistantAuthMode::ApiKey;
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::LocalSettingsStore;
    use crate::adapters::outbound::secrets::{InMemorySecrets, SecretStore, SettingsFile};
    use crate::application::ports::ChatGptStore;
    use crate::domain::chatgpt::{
        AssistantAuthMode, ChatGptAccountState, ChatGptPreferences, ChatGptRegistration,
        ChatGptTokens,
    };
    use serde_json::Value;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::Arc;
    use uuid::Uuid;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!("galpi-chatgpt-settings-{}", Uuid::now_v7()))
    }

    fn registration(needs_consent: bool) -> Result<ChatGptRegistration, String> {
        ChatGptRegistration::new(
            "oaiapp_test".to_owned(),
            "subject-1".to_owned(),
            Some("user@example.com".to_owned()),
            needs_consent,
        )
        .map_err(|error| format!("{error:?}"))
    }

    fn tokens(refresh: &str, expires_at: u64) -> ChatGptTokens {
        ChatGptTokens {
            access_token: "access-value-1".to_owned(),
            refresh_token: refresh.to_owned(),
            scopes: vec!["chatgpt.tokens.use.direct".to_owned()],
            expires_at,
        }
    }

    fn store(path: PathBuf, secrets: Arc<dyn SecretStore>) -> LocalSettingsStore {
        LocalSettingsStore::with_secrets(path, secrets)
    }

    fn cleared(document: &Value, key: &str) -> bool {
        document.get(key).is_none_or(Value::is_null)
    }

    #[tokio::test]
    async fn a_signed_in_account_survives_a_restart_without_reading_any_secret() -> TestResult {
        // Given: an account signed in by one process
        let directory = directory();
        let path = directory.join("settings.json");
        let secrets = Arc::new(InMemorySecrets::default());
        store(path.clone(), secrets.clone())
            .save_session(registration(false)?, Some(tokens("refresh-value-1", 9_000)))
            .await?;

        // When: the next launch asks what the sheet should show
        let restarted = store(path, secrets.clone());
        let settings = restarted.load_settings().await?;

        // Then: the answer comes from the settings file alone
        assert_eq!(settings.account.state, ChatGptAccountState::SignedIn);
        assert_eq!(settings.account.email.as_deref(), Some("user@example.com"));
        assert_eq!(secrets.reads(), 0);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn the_settings_file_never_carries_the_chatgpt_tokens() -> TestResult {
        // Given: a store that keeps secrets outside the settings file
        let directory = directory();
        let path = directory.join("settings.json");
        let store = store(path.clone(), Arc::new(InMemorySecrets::default()));

        // When: a session is saved and the tokens are rotated
        store
            .save_session(registration(false)?, Some(tokens("refresh-value-1", 9_000)))
            .await?;
        store
            .replace_tokens(tokens("refresh-value-2", 12_000))
            .await?;

        // Then: they read back, but the file on disk holds neither token
        assert_eq!(
            store.load_tokens().await?,
            Some(tokens("refresh-value-2", 12_000))
        );
        let document = tokio::fs::read_to_string(&path).await?;
        assert!(!document.contains("access-value-1"), "{document}");
        assert!(!document.contains("refresh-value-"), "{document}");
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn the_settings_file_backend_stores_private_tokens_and_sign_out_clears_parsed_values()
    -> TestResult {
        // Given: the settings file is the secret store, as it is today
        let directory = directory();
        let path = directory.join("settings.json");
        let store = store(path.clone(), Arc::new(SettingsFile));
        let host_id = store.load_or_create_host_id().await?;
        store
            .save_session(registration(false)?, Some(tokens("refresh-value-1", 9_000)))
            .await?;
        let stored: Value = serde_json::from_str(&tokio::fs::read_to_string(&path).await?)?;
        assert!(
            stored["chatgptTokens"]
                .as_str()
                .is_some_and(|tokens| tokens.contains("refresh-value-1"))
        );
        assert_eq!(
            tokio::fs::metadata(&path).await?.permissions().mode() & 0o777,
            0o600
        );

        // When
        store.clear_session().await?;

        // Then: parsed values are gone and the host id is untouched
        let after: Value = serde_json::from_str(&tokio::fs::read_to_string(&path).await?)?;
        for key in [
            "chatgptClientId",
            "chatgptTokens",
            "chatgptEmail",
            "chatgptSubject",
            "chatgptModel",
        ] {
            assert!(cleared(&after, key), "{key} survived: {after}");
        }
        assert_eq!(after["chatgptTokensStored"], Value::Bool(false));
        assert_eq!(after["chatgptAuthMode"], "apiKey");
        assert_eq!(after["chatgptHostId"], host_id.as_str());
        assert_eq!(store.load_tokens().await?, None);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn repeated_identical_chatgpt_tokens_are_not_rewritten() -> TestResult {
        // Given: tokens stored once
        let directory = directory();
        let secrets = Arc::new(InMemorySecrets::default());
        let store = store(directory.join("settings.json"), secrets.clone());
        store
            .save_session(registration(false)?, Some(tokens("refresh-value-1", 9_000)))
            .await?;
        assert_eq!(secrets.writes(), 1);

        // When: the same value arrives twice more
        store
            .replace_tokens(tokens("refresh-value-1", 9_000))
            .await?;
        store
            .save_session(registration(false)?, Some(tokens("refresh-value-1", 9_000)))
            .await?;

        // Then: the secret store was not asked again
        assert_eq!(secrets.writes(), 1);

        // And a rotated value is written
        store
            .replace_tokens(tokens("refresh-value-2", 12_000))
            .await?;
        assert_eq!(secrets.writes(), 2);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn saving_preferences_never_touches_the_secret_store() -> TestResult {
        // Given: a signed-in account
        let directory = directory();
        let secrets = Arc::new(InMemorySecrets::default());
        let store = store(directory.join("settings.json"), secrets.clone());
        store
            .save_session(registration(false)?, Some(tokens("refresh-value-1", 9_000)))
            .await?;
        let writes = secrets.writes();

        // When: the sheet autosaves twice
        let preferences = ChatGptPreferences {
            auth_mode: AssistantAuthMode::ChatGpt,
            model: Some("gpt-5".to_owned()),
            welcome_acknowledged: true,
        };
        store.save_preferences(preferences.clone()).await?;
        store.save_preferences(preferences).await?;

        // Then: no secret was read or written, and the choices persisted
        assert_eq!(secrets.writes(), writes);
        assert_eq!(secrets.reads(), 0);
        let settings = store.load_settings().await?;
        assert_eq!(settings.auth_mode, AssistantAuthMode::ChatGpt);
        assert_eq!(settings.model.as_deref(), Some("gpt-5"));
        assert!(settings.welcome_acknowledged);
        assert_eq!(settings.account.state, ChatGptAccountState::SignedIn);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn a_corrupted_settings_file_is_reported_rather_than_reset() -> TestResult {
        // Given
        let directory = directory();
        tokio::fs::create_dir_all(&directory).await?;
        let path = directory.join("settings.json");
        tokio::fs::write(&path, "{ not json").await?;
        let store = store(path.clone(), Arc::new(InMemorySecrets::default()));

        // When
        let result = store.load_settings().await;

        // Then
        assert_eq!(
            result.err().map(|error| error.code),
            Some("SETTINGS_INVALID".to_owned())
        );
        assert!(tokio::fs::try_exists(&path).await?);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn clearing_tokens_keeps_the_registration_and_asks_for_a_new_sign_in() -> TestResult {
        // Given
        let directory = directory();
        let store = store(
            directory.join("settings.json"),
            Arc::new(InMemorySecrets::default()),
        );
        store
            .save_session(registration(false)?, Some(tokens("refresh-value-1", 9_000)))
            .await?;

        // When
        store.clear_tokens().await?;

        // Then
        let settings = store.load_settings().await?;
        assert_eq!(settings.account.state, ChatGptAccountState::SignInRequired);
        assert_eq!(settings.account.email.as_deref(), Some("user@example.com"));
        assert_eq!(store.load_registration().await?, Some(registration(false)?));
        assert_eq!(store.load_tokens().await?, None);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn a_consent_registration_is_saved_without_tokens() -> TestResult {
        // Given
        let directory = directory();
        let store = store(
            directory.join("settings.json"),
            Arc::new(InMemorySecrets::default()),
        );

        // When: a grant without plan use stores only the registration
        store.save_session(registration(true)?, None).await?;

        // Then
        let loaded = store.load_registration().await?;
        assert_eq!(loaded.map(|value| value.needs_consent()), Some(true));
        assert_eq!(store.load_tokens().await?, None);
        assert_eq!(
            store.load_settings().await?.account.state,
            ChatGptAccountState::SignInRequired
        );
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn the_host_id_is_minted_once_and_survives_sign_out() -> TestResult {
        // Given
        let directory = directory();
        let store = store(
            directory.join("settings.json"),
            Arc::new(InMemorySecrets::default()),
        );
        let first = store.load_or_create_host_id().await?;
        store
            .save_session(registration(false)?, Some(tokens("refresh-value-1", 9_000)))
            .await?;

        // When
        store.clear_session().await?;

        // Then
        assert_eq!(store.load_or_create_host_id().await?, first);
        assert_eq!(
            store.load_settings().await?.account.state,
            ChatGptAccountState::SignedOut
        );
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }
}
