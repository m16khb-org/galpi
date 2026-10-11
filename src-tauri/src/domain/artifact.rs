use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Artifacts {
    pub srt: Option<PathBuf>,
    pub txt: PathBuf,
    pub checkpoint: Option<PathBuf>,
    pub minutes: Option<PathBuf>,
    pub output_directory: PathBuf,
    /// The audio this meeting was transcribed from, when there was any.
    ///
    /// Refinement dates the minutes from this file: it is the only artifact
    /// whose timestamp is the day the meeting happened rather than the day the
    /// transcript was written. An imported transcript has no audio.
    pub source_audio: Option<PathBuf>,
    /// The transcript file an import copied from, when the meeting was imported.
    ///
    /// Together with `source_audio` it names the meeting's original file, which
    /// stays the same however many output folders the meeting is written to.
    pub source_transcript: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Srt,
    SpeakerText,
    Checkpoint,
    Minutes,
}

impl Artifacts {
    pub fn path_for(&self, kind: ArtifactKind) -> Option<&PathBuf> {
        match kind {
            ArtifactKind::Srt => self.srt.as_ref(),
            ArtifactKind::SpeakerText => Some(&self.txt),
            ArtifactKind::Checkpoint => self.checkpoint.as_ref(),
            ArtifactKind::Minutes => self.minutes.as_ref(),
        }
    }
}

/// Sibling meeting-minutes path for a speaker-labeled transcript.
pub fn minutes_path(transcript: &Path) -> PathBuf {
    let stem = transcript.file_stem().map_or_else(
        || "meeting".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let base = stem.strip_suffix("_화자별").unwrap_or(stem.as_str());
    transcript.with_file_name(format!("{base}_회의록.md"))
}

#[cfg(test)]
mod tests {
    use super::minutes_path;
    use crate::domain::artifact::{ArtifactKind, Artifacts};
    use std::path::{Path, PathBuf};

    #[test]
    fn derives_minutes_name_from_the_speaker_transcript() {
        // Given
        let transcript = Path::new("/tmp/job/meeting_화자별.txt");

        // When
        let minutes = minutes_path(transcript);

        // Then
        assert_eq!(minutes, PathBuf::from("/tmp/job/meeting_회의록.md"));
    }

    #[test]
    fn keeps_the_stem_when_the_speaker_suffix_is_absent() {
        // Given
        let transcript = Path::new("/tmp/job/notes.txt");

        // When
        let minutes = minutes_path(transcript);

        // Then
        assert_eq!(minutes, PathBuf::from("/tmp/job/notes_회의록.md"));
    }

    #[test]
    fn imported_transcripts_have_no_optional_artifacts() {
        // Given: a transcript imported without running a transcription job
        let artifacts = Artifacts {
            srt: None,
            txt: PathBuf::from("/tmp/job/notes.txt"),
            checkpoint: None,
            minutes: None,
            output_directory: PathBuf::from("/tmp/job"),
            source_audio: None,
            source_transcript: Some(PathBuf::from("/tmp/notes.txt")),
        };

        // When / Then: only the transcript itself is addressable
        assert_eq!(
            artifacts.path_for(ArtifactKind::SpeakerText),
            Some(&PathBuf::from("/tmp/job/notes.txt"))
        );
        assert_eq!(artifacts.path_for(ArtifactKind::Srt), None);
        assert_eq!(artifacts.path_for(ArtifactKind::Checkpoint), None);
        assert_eq!(artifacts.path_for(ArtifactKind::Minutes), None);
    }
}
