"""WhisperX ASR on Apple Silicon through MLX.

CTranslate2, which WhisperX decodes with, has no Metal backend. On macOS the
ASR stage alone runs the same `large-v3-turbo` weights through `mlx-whisper`;
voice activity detection, chunking, alignment and diarization stay the
official WhisperX code, so every platform shares one pipeline and one segment
shape: one segment per VAD chunk of at most 30 seconds.
"""

from __future__ import annotations

from collections.abc import Callable, Sequence
from typing import Protocol, TypedDict, cast

from .artifacts import Segment, Transcription

MLX_WHISPER_MODEL_ID = "mlx-community/whisper-large-v3-turbo"
SAMPLE_RATE = 16_000
CHUNK_SECONDS = 30
# Same VAD thresholds and decoder guards the CTranslate2 path is given.
VAD_ONSET = 0.6
VAD_OFFSET = 0.4
NO_SPEECH_THRESHOLD = 0.75


class MlxSegment(TypedDict, total=False):
    text: str
    avg_logprob: float


class MlxResult(TypedDict, total=False):
    text: str
    segments: list[MlxSegment]


class VadChunk(TypedDict):
    start: float
    end: float


class AudioSamples(Protocol):
    """16 kHz mono samples; a numpy array in production."""

    def __getitem__(self, index: slice, /) -> object: ...


class MlxWhisper(Protocol):
    def transcribe(self, audio: object, **options: object) -> dict[str, object]: ...


def chunk_segment(start: float, end: float, result: MlxResult) -> Segment | None:
    """Fold one chunk's decoder output into a single WhisperX-shaped segment.

    The chunk carries its own timing, as WhisperX's batched pipeline does;
    alignment later assigns word times inside it. A chunk that decoded to
    nothing yields no segment.
    """

    pieces = result.get("segments") or []
    text = "".join(piece.get("text", "") for piece in pieces).strip()
    if not text:
        text = result.get("text", "").strip()
    if not text:
        return None
    segment = Segment(start=round(start, 3), end=round(end, 3), text=text)
    logprobs = [piece["avg_logprob"] for piece in pieces if "avg_logprob" in piece]
    if logprobs:
        segment["avg_logprob"] = sum(logprobs) / len(logprobs)
    return segment


def decode_chunks(
    audio: AudioSamples,
    chunks: Sequence[VadChunk],
    decoder: MlxWhisper,
    *,
    initial_prompt: str | None,
    progress: Callable[[float], None],
) -> Transcription:
    """Decode every VAD chunk with MLX Whisper, in order."""

    segments: list[Segment] = []
    total = len(chunks)
    for index, chunk in enumerate(chunks, start=1):
        start, end = chunk["start"], chunk["end"]
        raw = decoder.transcribe(
            audio[int(start * SAMPLE_RATE) : int(end * SAMPLE_RATE)],
            path_or_hf_repo=MLX_WHISPER_MODEL_ID,
            language="ko",
            task="transcribe",
            condition_on_previous_text=False,
            no_speech_threshold=NO_SPEECH_THRESHOLD,
            initial_prompt=initial_prompt,
            word_timestamps=False,
            verbose=None,
        )
        segment = chunk_segment(start, end, cast(MlxResult, cast(object, raw)))
        if segment is not None:
            segments.append(segment)
        progress(index / total * 100)
    return Transcription(segments=segments, language="ko")


def transcribe_mlx(
    audio: AudioSamples,
    *,
    initial_prompt: str | None,
    progress: Callable[[float], None],
) -> Transcription:
    """VAD with WhisperX's pyannote model, then MLX Whisper per chunk."""

    import mlx_whisper
    import torch
    from whisperx.vads import Pyannote

    vad = Pyannote(
        torch.device("cpu"),
        token=None,
        vad_onset=VAD_ONSET,
        vad_offset=VAD_OFFSET,
        chunk_size=CHUNK_SECONDS,
    )
    scores = vad(
        {"waveform": Pyannote.preprocess_audio(audio), "sample_rate": SAMPLE_RATE}
    )
    chunks = cast(
        list[VadChunk],
        Pyannote.merge_chunks(
            scores, CHUNK_SECONDS, onset=VAD_ONSET, offset=VAD_OFFSET
        ),
    )
    return decode_chunks(
        audio,
        chunks,
        cast(MlxWhisper, cast(object, mlx_whisper)),
        initial_prompt=initial_prompt,
        progress=progress,
    )
