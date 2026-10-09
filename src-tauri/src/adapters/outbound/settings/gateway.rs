//! The auth-gateway session on top of the shared settings document.
//!
//! The email is a plain field of `settings.json`. The token record is a
//! secret and rides the same `SecretStore` path as the other credentials.

use super::super::secrets::Secret;
use super::LocalSettingsStore;
use crate::application::error::AppError;
use crate::application::ports::GatewaySessionStore;
use crate::domain::gateway::GatewayTokens;
use async_trait::async_trait;

impl LocalSettingsStore {
    async fn store_gateway_tokens(&self, tokens: &GatewayTokens) -> Result<(), AppError> {
        let record = serde_json::to_string(tokens).map_err(|error| {
            AppError::new(
                "SETTINGS_SERIALIZE_ERROR",
                format!("로그인 정보를 저장 형식으로 바꾸지 못했습니다: {error}"),
            )
        })?;
        self.store_secret(Secret::GatewaySession, Some(&record))
            .await
    }
}

#[async_trait]
impl GatewaySessionStore for LocalSettingsStore {
    async fn load_email(&self) -> Result<Option<String>, AppError> {
        let state = self.load().await?;
        Ok(state
            .as_ref()
            .and_then(|settings| settings.gateway_email.clone()))
    }

    /// A record that no longer parses is as good as none: signing in again
    /// replaces it, and refusing to start would strand the app.
    async fn load_tokens(&self) -> Result<Option<GatewayTokens>, AppError> {
        Ok(self
            .secret(Secret::GatewaySession)
            .await?
            .and_then(|record| serde_json::from_str(&record).ok()))
    }

    async fn save_session(
        &self,
        tokens: GatewayTokens,
        email: Option<String>,
    ) -> Result<(), AppError> {
        self.update(|settings| settings.gateway_email = email)
            .await?;
        self.store_gateway_tokens(&tokens).await
    }

    async fn replace_tokens(&self, tokens: GatewayTokens) -> Result<(), AppError> {
        self.store_gateway_tokens(&tokens).await
    }

    /// Tokens first, so an interruption leaves the app signed out.
    async fn clear(&self) -> Result<(), AppError> {
        self.store_secret(Secret::GatewaySession, None).await?;
        self.update(|settings| settings.gateway_email = None).await
    }
}

#[cfg(test)]
mod tests {
    use super::LocalSettingsStore;
    use crate::adapters::outbound::secrets::{InMemorySecrets, SecretStore, SettingsFile};
    use crate::application::ports::GatewaySessionStore;
    use crate::domain::gateway::GatewayTokens;
    use std::path::PathBuf;
    use std::sync::Arc;
    use uuid::Uuid;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!("galpi-gateway-settings-{}", Uuid::now_v7()))
    }

    fn tokens(refresh: &str) -> GatewayTokens {
        GatewayTokens {
            access_token: "galpi-test-access".to_owned(),
            refresh_token: refresh.to_owned(),
            expires_at: 9_000,
        }
    }

    fn store(path: PathBuf, secrets: Arc<dyn SecretStore>) -> LocalSettingsStore {
        LocalSettingsStore::with_secrets(path, secrets)
    }

    #[tokio::test]
    async fn the_settings_file_backend_keeps_the_gateway_session_across_a_restart() -> TestResult {
        // Given: an empty install where the settings file is the secret store
        let directory = directory();
        let path = directory.join("settings.json");
        store(path.clone(), Arc::new(SettingsFile))
            .save_session(
                tokens("galpi-test-refresh-1"),
                Some("user@example.com".to_owned()),
            )
            .await?;

        // When: the next launch reads it back
        let restarted = store(path.clone(), Arc::new(SettingsFile));

        // Then
        assert_eq!(
            restarted.load_tokens().await?,
            Some(tokens("galpi-test-refresh-1"))
        );
        assert_eq!(
            restarted.load_email().await?.as_deref(),
            Some("user@example.com")
        );
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(
                &tokio::fs::metadata(&path).await?.permissions()
            ) & 0o777,
            0o600
        );
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn an_os_secret_store_keeps_the_gateway_tokens_out_of_the_settings_file() -> TestResult {
        // Given: a store that keeps secrets outside the settings file
        let directory = directory();
        let path = directory.join("settings.json");
        let secrets = Arc::new(InMemorySecrets::default());
        store(path.clone(), secrets.clone())
            .save_session(
                tokens("galpi-test-refresh-1"),
                Some("user@example.com".to_owned()),
            )
            .await?;

        // When: the next launch reads it back
        let restarted = store(path.clone(), secrets);

        // Then: both survive, and the file on disk holds no token
        assert_eq!(
            restarted.load_email().await?.as_deref(),
            Some("user@example.com")
        );
        assert_eq!(
            restarted.load_tokens().await?,
            Some(tokens("galpi-test-refresh-1"))
        );
        let document = tokio::fs::read_to_string(&path).await?;
        assert!(!document.contains("galpi-test"), "{document}");
        assert!(
            document.contains("\"gatewaySessionStored\":true"),
            "{document}"
        );
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn clearing_the_gateway_session_removes_an_otherwise_empty_settings_file() -> TestResult {
        // Given: a session is the only thing on file
        let directory = directory();
        let path = directory.join("settings.json");
        let store = store(path.clone(), Arc::new(SettingsFile));
        store
            .save_session(
                tokens("galpi-test-refresh-1"),
                Some("user@example.com".to_owned()),
            )
            .await?;
        assert!(tokio::fs::try_exists(&path).await?);

        // When
        store.clear().await?;

        // Then
        assert_eq!(store.load_tokens().await?, None);
        assert_eq!(store.load_email().await?, None);
        assert!(!tokio::fs::try_exists(&path).await?);
        let _removed = tokio::fs::remove_dir_all(directory).await;
        Ok(())
    }
}
