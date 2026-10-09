use super::paths::{AppPaths, canonical_blocking};
use super::platform::{Os, home_directory};
use crate::application::error::AppError;
use std::io::{Error, ErrorKind};
use std::path::Path;

const MODEL_DIRECTORIES: [&str; 3] = [
    "models--mobiuslabsgmbh--faster-whisper-large-v3-turbo",
    "models--kresnik--wav2vec2-large-xlsr-korean",
    "models--pyannote--speaker-diarization-community-1",
];

pub async fn import_standard_cache(paths: &AppPaths) -> Result<usize, AppError> {
    let home = home_directory(Os::current(), &|key| std::env::var_os(key));
    let source = home.join(".cache/huggingface/hub");
    let destination = paths.cache.join("huggingface/hub");
    tokio::task::spawn_blocking(move || import_models(&source, &destination))
        .await
        .map_err(|error| AppError::new("MODEL_CACHE_IMPORT_FAILED", error.to_string()))?
        .map_err(|error| AppError::io("기존 Hugging Face 모델 캐시를 가져오지 못했습니다", &error))
}

pub fn can_use_offline_cache(imported: usize, token: Option<&str>) -> bool {
    imported == MODEL_DIRECTORIES.len() && token.is_none_or(|token| token.trim().is_empty())
}

fn import_models(source_hub: &Path, destination_hub: &Path) -> Result<usize, Error> {
    if !source_hub.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(destination_hub)?;
    let mut imported = 0;
    for directory in MODEL_DIRECTORIES {
        let source = source_hub.join(directory);
        if !source.is_dir() {
            continue;
        }
        let canonical_source = canonical_blocking(&source)?;
        copy_tree(&source, &destination_hub.join(directory), &canonical_source)?;
        imported += 1;
    }
    Ok(imported)
}

fn copy_tree(source: &Path, destination: &Path, source_root: &Path) -> Result<(), Error> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_tree(&source_path, &destination_path, source_root)?;
        } else if file_type.is_symlink() {
            copy_symlink(&source_path, &destination_path, source_root)?;
        } else if file_type.is_file() {
            copy_file(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn copy_symlink(source: &Path, destination: &Path, source_root: &Path) -> Result<(), Error> {
    if std::fs::symlink_metadata(destination).is_ok() {
        return Ok(());
    }
    let resolved = canonical_blocking(source)?;
    if !resolved.starts_with(source_root) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "model cache symlink escapes its repository",
        ));
    }
    link_or_copy(source, destination, &resolved)
}

/// Keep the repository's relative symlink layout on unix.
#[cfg(unix)]
fn link_or_copy(source: &Path, destination: &Path, _resolved: &Path) -> Result<(), Error> {
    let target = std::fs::read_link(source)?;
    std::os::unix::fs::symlink(target, destination)
}

/// Creating symlinks on Windows needs a privilege ordinary users lack, so the
/// already-contained target is hard-linked or copied instead.
#[cfg(windows)]
fn link_or_copy(_source: &Path, destination: &Path, resolved: &Path) -> Result<(), Error> {
    copy_file(resolved, destination)
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), Error> {
    if destination.is_file() {
        return Ok(());
    }
    match std::fs::hard_link(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => std::fs::copy(source, destination).map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::{MODEL_DIRECTORIES, can_use_offline_cache, import_models};
    use uuid::Uuid;

    #[cfg(unix)]
    #[test]
    fn imports_fixed_model_cache_with_hard_links_and_safe_symlinks()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("galpi-model-cache-{}", Uuid::now_v7()));
        let source_hub = root.join("source");
        let destination_hub = root.join("destination");
        let model = source_hub.join(MODEL_DIRECTORIES[0]);
        let blob = model.join("blobs/model-data");
        let snapshot = model.join("snapshots/revision");
        std::fs::create_dir_all(&snapshot)?;
        std::fs::create_dir_all(blob.parent().ok_or("blob parent missing")?)?;
        std::fs::write(&blob, b"model")?;
        std::os::unix::fs::symlink("../../blobs/model-data", snapshot.join("config.json"))?;

        let imported = import_models(&source_hub, &destination_hub)?;

        let imported_blob = destination_hub
            .join(MODEL_DIRECTORIES[0])
            .join("blobs/model-data");
        let imported_link = destination_hub
            .join(MODEL_DIRECTORIES[0])
            .join("snapshots/revision/config.json");
        assert_eq!(imported, 1);
        assert_eq!(std::fs::read(&imported_link)?, b"model");
        assert_eq!(
            std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&blob)?),
            std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&imported_blob)?)
        );
        assert!(
            std::fs::symlink_metadata(imported_link)?
                .file_type()
                .is_symlink()
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn imports_fixed_model_cache_by_hard_linking_or_copying_files()
    -> Result<(), Box<dyn std::error::Error>> {
        // Given: a snapshot entry that is a regular file standing in for a link
        let root = std::env::temp_dir().join(format!("galpi-model-cache-{}", Uuid::now_v7()));
        let source_hub = root.join("source");
        let destination_hub = root.join("destination");
        let snapshot = source_hub
            .join(MODEL_DIRECTORIES[0])
            .join("snapshots/revision");
        std::fs::create_dir_all(&snapshot)?;
        std::fs::write(snapshot.join("config.json"), b"model")?;

        // When
        let imported = import_models(&source_hub, &destination_hub)?;

        // Then
        assert_eq!(imported, 1);
        let copied = destination_hub
            .join(MODEL_DIRECTORIES[0])
            .join("snapshots/revision/config.json");
        assert_eq!(std::fs::read(copied)?, b"model");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn uses_offline_mode_only_for_complete_tokenless_cache() {
        assert!(can_use_offline_cache(MODEL_DIRECTORIES.len(), None));
        assert!(can_use_offline_cache(MODEL_DIRECTORIES.len(), Some("  ")));
        assert!(!can_use_offline_cache(MODEL_DIRECTORIES.len() - 1, None));
        assert!(!can_use_offline_cache(
            MODEL_DIRECTORIES.len(),
            Some("hf_token")
        ));
    }
}
