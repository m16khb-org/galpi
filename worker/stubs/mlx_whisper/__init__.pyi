import numpy as np

def transcribe(
    audio: np.ndarray,
    *,
    path_or_hf_repo: str = ...,
    verbose: bool | None = ...,
    condition_on_previous_text: bool = ...,
    no_speech_threshold: float | None = ...,
    initial_prompt: str | None = ...,
    word_timestamps: bool = ...,
    **decode_options: object,
) -> dict[str, object]: ...
