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
    /// The meeting's source recording. Only its Drive copy is named by this
    /// kind; it is not a result file, so it has no path in [`Artifacts`].
    Recording,
}

impl Artifacts {
    pub fn path_for(&self, kind: ArtifactKind) -> Option<&PathBuf> {
        match kind {
            ArtifactKind::Srt => self.srt.as_ref(),
            ArtifactKind::SpeakerText => Some(&self.txt),
            ArtifactKind::Checkpoint => self.checkpoint.as_ref(),
            ArtifactKind::Minutes => self.minutes.as_ref(),
            ArtifactKind::Recording => None,
        }
    }
}

/// The suffix Galpi gives the speaker-labeled transcript of a recording.
const SPEAKER_TEXT_SUFFIX: &str = "_화자별";

/// The recording's name behind a speaker transcript's file stem:
/// `새로운 노트_화자별` is `새로운 노트`. Any other stem, or one that would be
/// left empty, is returned unchanged.
pub fn recording_name(stem: &str) -> &str {
    stem.strip_suffix(SPEAKER_TEXT_SUFFIX)
        .filter(|base| !base.trim().is_empty())
        .unwrap_or(stem)
}

/// Whether a file name is one a transcript import accepts (`.txt` or `.md`).
/// Works on the name as text, so a remote name is read the same on every OS.
pub fn is_transcript_name(name: &str) -> bool {
    matches!(
        name.rsplit_once('.'),
        Some((stem, "txt" | "md")) if !stem.is_empty()
    )
}

/// Sibling meeting-minutes path for a speaker-labeled transcript.
pub fn minutes_path(transcript: &Path) -> PathBuf {
    let stem = transcript.file_stem().map_or_else(
        || "meeting".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    transcript.with_file_name(format!("{}_회의록.md", recording_name(&stem)))
}

#[cfg(test)]
mod tests {
    use super::{is_transcript_name, minutes_path, recording_name};
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
    fn a_speaker_transcript_stem_names_its_recording() {
        assert_eq!(recording_name("새로운 노트_화자별"), "새로운 노트");
        assert_eq!(recording_name("새로운 노트"), "새로운 노트");
        assert_eq!(recording_name("_화자별"), "_화자별");
    }

    #[test]
    fn transcript_names_are_txt_or_md_with_a_stem() {
        assert!(is_transcript_name("새로운 노트_화자별.txt"));
        assert!(is_transcript_name("notes.md"));
        for refused in ["녹음.m4a", ".txt", "memo", "notes.txt.bak", "notes.TXT"] {
            assert!(!is_transcript_name(refused), "{refused}");
        }
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
        assert_eq!(artifacts.path_for(ArtifactKind::Recording), None);
    }
}
