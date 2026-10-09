from collections.abc import Callable, Sequence

from .schema import AlignedTranscriptionResult, SingleSegment, TranscriptionResult

class Audio:
    def __len__(self) -> int: ...

class WhisperModel:
    def transcribe(
        self,
        audio: Audio,
        *,
        batch_size: int,
        language: str,
        progress_callback: Callable[[float], None] | None = None,
    ) -> TranscriptionResult: ...

def load_audio(file: str) -> Audio: ...
def load_model(
    model_name: str,
    device: str,
    *,
    compute_type: str,
    language: str,
    asr_options: dict[str, object],
    vad_options: dict[str, object],
) -> WhisperModel: ...
def load_align_model(*, language_code: str, device: str) -> tuple[object, object]: ...
def align(
    segments: Sequence[SingleSegment],
    model: object,
    metadata: object,
    audio: Audio,
    device: str,
    *,
    progress_callback: Callable[[float], None] | None = None,
) -> AlignedTranscriptionResult: ...
