use super::environment::{ALIGN_MODEL_DIR, PYANNOTE_MODEL_DIR, whisperx_asr_model_dir};
use super::paths::{AppPaths, canonical_blocking};
use super::platform::{Os, home_directory};
use crate::application::error::AppError;
use std::io::{Error, ErrorKind};
use std::path::Path;

/// The Hugging Face cache directories one `WhisperX` preparation downloads.
const fn model_directories(os: Os) -> [&'static str; 3] {
    [
        whisperx_asr_model_dir(os),
        ALIGN_MODEL_DIR,
        PYANNOTE_MODEL_DIR,
    ]
}

/// How many directories a complete cache holds, whatever the OS.
const MODEL_COUNT: usize = model_directories(Os::MacOs).len();

pub async fn import_standard_cache(paths: &AppPaths) -> Result<usize, AppError> {
    let os = Os::current();
    let home = home_directory(os, &|key| std::env::var_os(key));
    let source = home.join(".cache/huggingface/hub");
    let destination = paths.cache.join("huggingface/hub");
    tokio::task::spawn_blocking(move || import_models(&source, &destination, os))
        .await
        .map_err(|error| AppError::new("MODEL_CACHE_IMPORT_FAILED", error.to_string()))?
        .map_err(|error| AppError::io("기존 Hugging Face 모델 캐시를 가져오지 못했습니다", &error))
}

pub fn can_use_offline_cache(imported: usize) -> bool {
    imported == MODEL_COUNT
}

fn import_models(source_hub: &Path, destination_hub: &Path, os: Os) -> Result<usize, Error> {
    if !source_hub.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(destination_hub)?;
    let mut imported = 0;
    for directory in model_directories(os) {
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
    use super::{MODEL_COUNT, can_use_offline_cache, import_models, model_directories};
    use crate::adapters::outbound::platform::Os;
    use uuid::Uuid;

    /// The ASR directory of an arbitrary OS; the import tests only need one.
    fn asr_directory() -> &'static str {
        model_directories(Os::MacOs)[0]
    }

    #[cfg(unix)]
    #[test]
    fn imports_fixed_model_cache_with_hard_links_and_safe_symlinks()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("galpi-model-cache-{}", Uuid::now_v7()));
        let source_hub = root.join("source");
        let destination_hub = root.join("destination");
        let model = source_hub.join(asr_directory());
        let blob = model.join("blobs/model-data");
        let snapshot = model.join("snapshots/revision");
        std::fs::create_dir_all(&snapshot)?;
        std::fs::create_dir_all(blob.parent().ok_or("blob parent missing")?)?;
        std::fs::write(&blob, b"model")?;
        std::os::unix::fs::symlink("../../blobs/model-data", snapshot.join("config.json"))?;

        let imported = import_models(&source_hub, &destination_hub, Os::MacOs)?;

        let imported_blob = destination_hub
            .join(asr_directory())
            .join("blobs/model-data");
        let imported_link = destination_hub
            .join(asr_directory())
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
        let snapshot = source_hub.join(asr_directory()).join("snapshots/revision");
        std::fs::create_dir_all(&snapshot)?;
        std::fs::write(snapshot.join("config.json"), b"model")?;

        // When
        let imported = import_models(&source_hub, &destination_hub, Os::MacOs)?;

        // Then
        assert_eq!(imported, 1);
        let copied = destination_hub
            .join(asr_directory())
            .join("snapshots/revision/config.json");
        assert_eq!(std::fs::read(copied)?, b"model");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn uses_offline_mode_only_for_a_complete_cache() {
        assert!(can_use_offline_cache(MODEL_COUNT));
        assert!(!can_use_offline_cache(MODEL_COUNT - 1));
    }

    #[test]
    fn each_os_imports_only_its_own_asr_cache() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a standard cache holding the MLX ASR model and the shared ones
        let root = std::env::temp_dir().join(format!("galpi-model-cache-{}", Uuid::now_v7()));
        let source_hub = root.join("source");
        for directory in model_directories(Os::MacOs) {
            std::fs::create_dir_all(source_hub.join(directory))?;
        }

        // When / Then: macOS finds all three, Windows misses its faster-whisper one
        assert_eq!(
            import_models(&source_hub, &root.join("mac"), Os::MacOs)?,
            MODEL_COUNT
        );
        assert_eq!(
            import_models(&source_hub, &root.join("windows"), Os::Windows)?,
            MODEL_COUNT - 1
        );
        assert_ne!(
            model_directories(Os::MacOs)[0],
            model_directories(Os::Windows)[0]
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
