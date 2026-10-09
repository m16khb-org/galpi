from typing import Literal, overload

from .schema import AlignedTranscriptionResult, TranscriptionResult

class DiarizationSegments: ...

class DiarizationPipeline:
    def __init__(
        self, *, model_name: str, device: str, token: str | None = ...
    ) -> None: ...
    @overload
    def __call__(
        self,
        audio: object,
        *,
        num_speakers: int | None = ...,
        min_speakers: int | None = ...,
        max_speakers: int | None = ...,
        return_embeddings: Literal[False] = ...,
    ) -> DiarizationSegments: ...
    @overload
    def __call__(
        self,
        audio: object,
        *,
        num_speakers: int | None = ...,
        min_speakers: int | None = ...,
        max_speakers: int | None = ...,
        return_embeddings: Literal[True],
    ) -> tuple[DiarizationSegments, dict[str, list[float]] | None]: ...

def assign_word_speakers(
    diarize_df: DiarizationSegments,
    transcript_result: AlignedTranscriptionResult | TranscriptionResult,
) -> AlignedTranscriptionResult | TranscriptionResult: ...
