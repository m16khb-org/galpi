from typing import Literal, overload

from pyannote.audio import Pipeline

class DiarizationSegments:
    def to_dict(self, orient: Literal["records"]) -> list[dict[str, object]]: ...

class DiarizationPipeline:
    model: Pipeline
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
