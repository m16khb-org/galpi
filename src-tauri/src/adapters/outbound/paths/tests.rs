use super::{
    AppPaths, canonical, canonical_blocking, prepare_job_directory, sanitize_name, uv_binary_for,
};
use crate::adapters::outbound::platform::Os;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn temp_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("galpi-{label}-{}", Uuid::now_v7()))
}

#[tokio::test]
async fn creates_a_meeting_folder_named_after_the_recording()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: an imported audio file
    let root = temp_root("folder");
    let input = root.join("팀 미팅.m4a");
    let output = root.join("galpi");
    std::fs::create_dir_all(&root)?;
    std::fs::write(&input, b"audio")?;

    // When
    let (_input, job) = prepare_job_directory(&input, &output).await?;

    // Then: the meeting folder carries the recording name, no uuid suffix
    assert_eq!(job, canonical_blocking(&output.join("팀 미팅"))?);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn reuses_the_folder_a_recording_already_lives_in() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a Galpi recording inside its own meeting folder
    let root = temp_root("reuse");
    let output = root.join("galpi");
    let folder = output.join("2026-08-24 143052 녹음");
    std::fs::create_dir_all(&folder)?;
    let input = folder.join("2026-08-24 143052 녹음.wav");
    std::fs::write(&input, b"audio")?;

    // When
    let (_input, job) = prepare_job_directory(&input, &output).await?;

    // Then: transcription targets the recording's own folder
    assert_eq!(job, canonical_blocking(&folder)?);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn deduplicates_colliding_meeting_folders() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a meeting folder with the same name already exists
    let root = temp_root("dedup");
    let input = root.join("meeting.wav");
    let output = root.join("galpi");
    std::fs::create_dir_all(output.join("meeting"))?;
    std::fs::write(&input, b"audio")?;

    // When
    let (_input, job) = prepare_job_directory(&input, &output).await?;

    // Then
    assert_eq!(job, canonical_blocking(&output.join("meeting 2"))?);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn seeds_new_job_with_latest_matching_checkpoint() -> Result<(), Box<dyn std::error::Error>> {
    // Given: an earlier meeting for the same audio name holds a checkpoint
    let root = temp_root("checkpoint");
    let input = root.join("meeting.wav");
    let output = root.join("galpi");
    let previous = output.join("meeting 2");
    std::fs::create_dir_all(&previous)?;
    std::fs::write(&input, b"audio")?;
    std::fs::write(
        previous.join("meeting.aligned.v2.json"),
        b"{\"segments\":[]}",
    )?;

    // When
    let (_input, job) = prepare_job_directory(&input, &output).await?;

    // Then: the checkpoint is reused even though the folder names differ
    assert_eq!(
        std::fs::read(job.join("meeting.aligned.v2.json"))?,
        b"{\"segments\":[]}"
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn sanitize_name_keeps_spaces_and_hangul() {
    assert_eq!(sanitize_name(" 팀 미팅: 8월 "), "팀 미팅- 8월");
}

#[test]
fn windows_paths_use_the_scripts_directory_and_documents_galpi() {
    // Given / When
    let paths = AppPaths::from_roots(Os::Windows, PathBuf::from("/data"), Path::new("/documents"));

    // Then
    assert_eq!(
        paths.python,
        PathBuf::from("/data/engine/.venv/Scripts/python.exe")
    );
    assert_eq!(
        paths.qwen3_python,
        PathBuf::from("/data/engine/qwen3/.venv/Scripts/python.exe")
    );
    assert_eq!(paths.default_output, PathBuf::from("/documents/Galpi"));
}

#[test]
fn macos_paths_keep_the_posix_interpreter_layout() {
    let paths = AppPaths::from_roots(Os::MacOs, PathBuf::from("/data"), Path::new("/docs"));
    assert_eq!(paths.python, PathBuf::from("/data/engine/.venv/bin/python"));
    assert_eq!(paths.default_output, PathBuf::from("/docs/Galpi"));
}

#[test]
fn the_uv_binary_is_staged_in_debug_and_installed_beside_the_executable() {
    let manifest = Path::new("/repo/src-tauri");
    let executable = Path::new("/apps/Galpi/galpi");
    for (os, staged, installed) in [
        (Os::MacOs, "uv-aarch64-apple-darwin", "uv"),
        (Os::Windows, "uv-x86_64-pc-windows-msvc.exe", "uv.exe"),
    ] {
        assert_eq!(
            uv_binary_for(os, manifest, None).ok(),
            Some(manifest.join("binaries").join(staged))
        );
        assert_eq!(
            uv_binary_for(os, manifest, Some(executable)).ok(),
            Some(PathBuf::from("/apps/Galpi").join(installed))
        );
    }
}

#[tokio::test]
async fn canonical_resolves_the_same_location_without_a_verbatim_prefix()
-> Result<(), Box<dyn std::error::Error>> {
    // Given
    let root = temp_root("canonical");
    std::fs::create_dir_all(&root)?;

    // When
    let resolved = canonical(&root).await?;
    let resolved_blocking = canonical_blocking(&root)?;

    // Then: same file as std's answer, but never the `\\?\` form Windows
    // returns, which worker argv and UI strings cannot use
    assert_eq!(resolved, resolved_blocking);
    assert_eq!(
        std::fs::canonicalize(&resolved)?,
        std::fs::canonicalize(&root)?
    );
    assert!(!resolved.to_string_lossy().starts_with(r"\\?\"));
    std::fs::remove_dir_all(root)?;
    Ok(())
}
