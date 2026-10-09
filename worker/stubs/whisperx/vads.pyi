from collections.abc import Mapping

import torch

class Pyannote:
    def __init__(
        self,
        device: torch.device,
        token: str | None = ...,
        model_fp: str | None = ...,
        *,
        vad_onset: float,
        vad_offset: float,
        chunk_size: int,
    ) -> None: ...
    def __call__(self, audio: Mapping[str, object]) -> object: ...
    @staticmethod
    def preprocess_audio(audio: object) -> torch.Tensor: ...
    @staticmethod
    def merge_chunks(
        segments: object,
        chunk_size: int,
        onset: float = ...,
        offset: float | None = ...,
    ) -> list[dict[str, object]]: ...
