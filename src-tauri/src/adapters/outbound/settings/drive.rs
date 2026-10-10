//! The Google Drive connection on top of the shared settings document.
//!
//! The account, the `Galpi` folder id, and the onboarding choice are plain
//! fields of `settings.json`. The token record is a secret and rides the same
//! `SecretStore` path as the other credentials.

use super::super::secrets::Secret;
use super::LocalSettingsStore;
use crate::application::error::AppError;
use crate::application::ports::DriveSessionStore;
use crate::domain::drive::{DriveConnection, DriveStatus, DriveTokens};
use async_trait::async_trait;

impl LocalSettingsStore {
    async fn store_drive_tokens(&self, tokens: &DriveTokens) -> Result<(), AppError> {
        let record = serde_json::to_string(tokens).map_err(|error| {
            AppError::new(
                "SETTINGS_SERIALIZE_ERROR",
                format!("Drive 연결 정보를 저장 형식으로 바꾸지 못했습니다: {error}"),
            )
        })?;
        self.store_secret(Secret::DriveSession, Some(&record)).await
    }
}

#[async_trait]
impl DriveSessionStore for LocalSettingsStore {
    async fn load_status(&self) -> Result<DriveStatus, AppError> {
        let state = self.load().await?;
        let settings = state.as_ref();
        Ok(
            match settings.and_then(|settings| settings.drive_email.clone()) {
                Some(email) => DriveStatus::Connected { email },
                None => DriveStatus::Disconnected {
                    onboarding_dismissed: settings
                        .is_some_and(|settings| settings.drive_onboarding_dismissed),
                },
            },
        )
    }

    async fn load_connection(&self) -> Result<Option<DriveConnection>, AppError> {
        let state = self.load().await?;
        Ok(state.as_ref().and_then(|settings| {
            Some(DriveConnection {
                email: settings.drive_email.clone()?,
                permission_id: settings.drive_permission_id.clone()?,
                folder_id: settings.drive_folder_id.clone()?,
            })
        }))
    }

    /// A record that no longer parses is as good as none: connecting again
    /// replaces it.
    async fn load_tokens(&self) -> Result<Option<DriveTokens>, AppError> {
        Ok(self
            .secret(Secret::DriveSession)
            .await?
            .and_then(|record| serde_json::from_str(&record).ok()))
    }

    /// Tokens first, so an interruption never shows a connection without them.
    async fn save_connection(
        &self,
        connection: DriveConnection,
        tokens: DriveTokens,
    ) -> Result<(), AppError> {
        self.store_drive_tokens(&tokens).await?;
        self.update(|settings| {
            settings.drive_email = Some(connection.email);
            settings.drive_permission_id = Some(connection.permission_id);
            settings.drive_folder_id = Some(connection.folder_id);
        })
        .await
    }

    async fn replace_tokens(&self, tokens: DriveTokens) -> Result<(), AppError> {
        self.store_drive_tokens(&tokens).await
    }

    async fn save_folder_id(&self, folder_id: String) -> Result<(), AppError> {
        self.update(|settings| settings.drive_folder_id = Some(folder_id))
            .await
    }

    async fn dismiss_onboarding(&self) -> Result<(), AppError> {
        self.update(|settings| settings.drive_onboarding_dismissed = true)
            .await
    }

    /// Tokens first, so an interruption leaves Drive disconnected.
    async fn clear(&self) -> Result<(), AppError> {
        self.store_secret(Secret::DriveSession, None).await?;
        self.update(|settings| {
            settings.drive_email = None;
            settings.drive_permission_id = None;
            settings.drive_folder_id = None;
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::LocalSettingsStore;
    use crate::adapters::outbound::secrets::{InMemorySecrets, SecretStore, SettingsFile};
    use crate::application::ports::DriveSessionStore;
    use crate::domain::drive::{DriveConnection, DriveStatus, DriveTokens};
    use std::path::PathBuf;
    use std::sync::Arc;
    use uuid::Uuid;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!("galpi-drive-settings-{}", Uuid::now_v7()))
    }

    fn tokens(refresh: &str) -> DriveTokens {
        DriveTokens {
            access_token: "galpi-test-access".to_owned(),
            refresh_token: refresh.to_owned(),
            expires_at: 9_000,
        }
    }

    fn connection() -> DriveConnection {
        DriveConnection {
            email: "user@example.com".to_owned(),
            permission_id: "perm-1".to_owned(),
            folder_id: "folder-1".to_owned(),
        }
    }

    fn store(path: PathBuf, secrets: Arc<dyn SecretStore>) -> LocalSettingsStore {
        LocalSettingsStore::with_secrets(path, secrets)
    }

    #[tokio::test]
    async fn the_settings_file_backend_keeps_the_drive_connection_across_a_restart() -> TestResult {
        // Given: an empty install where the settings file is the secret store
        let directory = directory();
        let path = directory.join("settings.json");
        store(path.clone(), Arc::new(SettingsFile))
            .save_connection(connection(), tokens("galpi-test-refresh-1"))
            .await?;

        // When: the next launch reads it back
        let restarted = store(path.clone(), Arc::new(SettingsFile));

        // Then
        assert_eq!(restarted.load_connection().await?, Some(connection()));
        assert_eq!(
            restarted.load_tokens().await?,
            Some(tokens("galpi-test-refresh-1"))
        );
        assert_eq!(
            restarted.load_status().await?,
            DriveStatus::Connected {
                email: "user@example.com".to_owned()
            }
        );
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn an_os_secret_store_keeps_the_drive_tokens_out_of_the_settings_file() -> TestResult {
        // Given: a store that keeps secrets outside the settings file
        let directory = directory();
        let path = directory.join("settings.json");
        let secrets = Arc::new(InMemorySecrets::default());
        store(path.clone(), secrets.clone())
            .save_connection(connection(), tokens("galpi-test-refresh-1"))
            .await?;

        // When: the next launch reads it back
        let restarted = store(path.clone(), secrets);

        // Then: both survive, and the file on disk holds no token
        assert_eq!(
            restarted.load_tokens().await?,
            Some(tokens("galpi-test-refresh-1"))
        );
        let document = tokio::fs::read_to_string(&path).await?;
        assert!(!document.contains("galpi-test"), "{document}");
        assert!(
            document.contains("\"driveSessionStored\":true"),
            "{document}"
        );
        assert!(
            document.contains("\"driveFolderId\":\"folder-1\""),
            "{document}"
        );
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn clearing_the_drive_connection_removes_an_otherwise_empty_settings_file() -> TestResult
    {
        // Given: a Drive connection is the only thing on file
        let directory = directory();
        let path = directory.join("settings.json");
        let store = store(path.clone(), Arc::new(SettingsFile));
        store
            .save_connection(connection(), tokens("galpi-test-refresh-1"))
            .await?;
        assert!(tokio::fs::try_exists(&path).await?);

        // When
        store.clear().await?;

        // Then
        assert_eq!(store.load_tokens().await?, None);
        assert_eq!(store.load_connection().await?, None);
        assert!(!tokio::fs::try_exists(&path).await?);
        let _removed = tokio::fs::remove_dir_all(directory).await;
        Ok(())
    }

    #[tokio::test]
    async fn the_onboarding_choice_outlives_a_disconnect() -> TestResult {
        // Given
        let directory = directory();
        let path = directory.join("settings.json");
        let store = store(path.clone(), Arc::new(SettingsFile));
        store.dismiss_onboarding().await?;
        store
            .save_connection(connection(), tokens("galpi-test-refresh-1"))
            .await?;

        // When
        store.clear().await?;
        store
            .save_folder_id("ignored-after-restart".to_owned())
            .await?;
        let restarted = LocalSettingsStore::with_secrets(path, Arc::new(SettingsFile));

        // Then: still disconnected (no account), and the card stays dismissed
        assert_eq!(
            restarted.load_status().await?,
            DriveStatus::Disconnected {
                onboarding_dismissed: true
            }
        );
        assert_eq!(restarted.load_connection().await?, None);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }

    #[tokio::test]
    async fn a_settings_file_from_before_drive_still_parses() -> TestResult {
        // Given: a document written by a build without any Drive field
        let directory = directory();
        let path = directory.join("settings.json");
        tokio::fs::create_dir_all(&directory).await?;
        tokio::fs::write(
            &path,
            r#"{"gatewayEmail":"user@example.com","gatewaySessionStored":false}"#,
        )
        .await?;

        // When
        let store = store(path, Arc::new(SettingsFile));

        // Then
        assert_eq!(
            store.load_status().await?,
            DriveStatus::Disconnected {
                onboarding_dismissed: false
            }
        );
        assert_eq!(store.load_tokens().await?, None);
        tokio::fs::remove_dir_all(directory).await?;
        Ok(())
    }
}
