use super::paths::{canonical, create_meeting_directory, meeting_stem, prepare_output_root};
use crate::application::error::AppError;
use crate::domain::artifact::Artifacts;
use std::path::Path;

/// Largest transcript accepted for import: text transcripts are far below this,
/// so anything bigger is a mistaken selection such as an audio file.
const MAX_TRANSCRIPT_BYTES: u64 = 20 * 1024 * 1024;

/// Copy an existing transcript into a per-meeting folder so refinement,
/// reveal, and minutes naming work exactly like a transcribed meeting.
pub async fn import_transcript(input: &Path, output_root: &Path) -> Result<Artifacts, AppError> {
    let input = canonical(input)
        .await
        .map_err(|error| AppError::io("가져온 전사문을 확인하지 못했습니다", &error))?;
    let metadata = tokio::fs::metadata(&input)
        .await
        .map_err(|error| AppError::io("가져온 전사문 정보를 읽지 못했습니다", &error))?;
    if !metadata.is_file() {
        return Err(AppError::new(
            "INVALID_TRANSCRIPT",
            "가져온 경로가 전사문 파일이 아닙니다.",
        ));
    }
    if !matches!(
        input.extension().and_then(|extension| extension.to_str()),
        Some("txt" | "md")
    ) {
        return Err(AppError::new(
            "INVALID_TRANSCRIPT",
            "txt 또는 md 텍스트 파일만 가져올 수 있습니다.",
        ));
    }
    if metadata.len() > MAX_TRANSCRIPT_BYTES {
        return Err(AppError::new(
            "TRANSCRIPT_TOO_LARGE",
            "전사문이 너무 큽니다. 20MB 이하의 텍스트 파일을 사용해 주세요.",
        ));
    }

    let output_root = prepare_output_root(output_root).await?;
    let stem = meeting_stem(&input);
    let directory = create_meeting_directory(input.parent(), &output_root, &stem).await?;
    let destination = directory.join(input.file_name().ok_or_else(|| {
        AppError::new("INVALID_TRANSCRIPT", "전사문 이름을 확인하지 못했습니다.")
    })?);
    let transcript = if destination == input {
        destination
    } else {
        tokio::fs::copy(&input, &destination)
            .await
            .map_err(|error| AppError::io("가져온 전사문을 저장하지 못했습니다", &error))?;
        canonical_transcript(&directory, &destination).await?
    };
    Ok(Artifacts {
        srt: None,
        txt: transcript,
        checkpoint: None,
        minutes: None,
        output_directory: directory,
        source_audio: None,
    })
}

async fn canonical_transcript(
    directory: &Path,
    transcript: &Path,
) -> Result<std::path::PathBuf, AppError> {
    let transcript = canonical(transcript)
        .await
        .map_err(|error| AppError::io("가져온 전사문을 확인하지 못했습니다", &error))?;
    if !transcript.starts_with(directory) {
        return Err(AppError::new(
            "OUTPUT_PATH_ERROR",
            "가져온 전사문이 회의 폴더를 벗어났습니다.",
        ));
    }
    Ok(transcript)
}

#[cfg(test)]
mod tests {
    use super::{canonical, import_transcript};

    #[tokio::test]
    async fn copies_transcript_into_a_meeting_folder() -> Result<(), Box<dyn std::error::Error>> {
        // Given: an existing transcript outside the Galpi folder
        let root = std::env::temp_dir().join(format!("galpi-import-{}", uuid::Uuid::now_v7()));
        let source_dir = root.join("source");
        std::fs::create_dir_all(&source_dir)?;
        let source = source_dir.join("팀미팅.txt");
        std::fs::write(&source, "스피커1: 안녕하세요")?;
        let output = root.join("galpi");

        // When
        let artifacts = import_transcript(&source, &output).await?;

        // Then: the copy lives in a folder named after the transcript
        assert_eq!(
            artifacts.txt,
            canonical(&output.join("팀미팅/팀미팅.txt")).await?
        );
        assert_eq!(
            artifacts.output_directory,
            canonical(&output.join("팀미팅")).await?
        );
        assert_eq!(artifacts.srt, None);
        assert_eq!(artifacts.checkpoint, None);
        assert_eq!(
            std::fs::read_to_string(artifacts.txt.clone())?,
            "스피커1: 안녕하세요"
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn rejects_non_text_files() -> Result<(), Box<dyn std::error::Error>> {
        // Given: an audio file offered as a transcript
        let root = std::env::temp_dir().join(format!("galpi-import-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root)?;
        let source = root.join("meeting.wav");
        std::fs::write(&source, b"audio")?;

        // When
        let Err(error) = import_transcript(&source, &root).await else {
            return Err("non-text transcript unexpectedly imported".into());
        };

        // Then
        assert_eq!(error.code, "INVALID_TRANSCRIPT");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
