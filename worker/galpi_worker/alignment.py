"""WhisperX word alignment with a bounded set of input shapes.

Heavy imports (torch, whisperx) stay inside functions.
"""

from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING, Protocol, cast

from .artifacts import Segment, Transcription
from .runtime import release_torch_cache

if TYPE_CHECKING:
    import torch
    from whisperx import Audio

SAMPLE_RATE = 16000
# Segment audio is zero-padded up to a multiple of this many seconds.
PAD_SECONDS = 1.0


class Wav2Vec2Config(Protocol):
    conv_kernel: list[int]
    conv_stride: list[int]


class Wav2Vec2Output(Protocol):
    logits: torch.Tensor


class Wav2Vec2ForCTC(Protocol):
    config: Wav2Vec2Config

    def __call__(
        self, input_values: torch.Tensor, *, attention_mask: torch.Tensor
    ) -> Wav2Vec2Output: ...


def output_frames(config: Wav2Vec2Config, samples: int) -> int:
    """Frames the wav2vec2 feature encoder emits for `samples` of audio."""

    frames = samples
    for kernel, stride in zip(config.conv_kernel, config.conv_stride, strict=True):
        frames = (frames - kernel) // stride + 1
    return frames


class PaddedAligner:
    """Run the CTC model on audio padded to a few fixed lengths.

    WhisperX feeds every segment at its own length. On MPS each new length
    keeps its own compiled graph and buffers that `empty_cache` does not
    return, so a 33-minute meeting held about 14 GB and the alignment, then
    the diarization after it, ran out of memory on a 16 GB Mac. Padding with
    an attention mask leaves the logits of the real frames as they were: the
    model normalizes per frame, masks attention, and zeroes padded frames
    before its positional convolution, so only the extra frames are new, and
    they are cut off.
    """

    def __init__(self, model: Wav2Vec2ForCTC) -> None:
        self.model = model

    def __call__(self, waveform: torch.Tensor) -> SimpleNamespace:
        import torch

        length = int(waveform.shape[-1])
        step = int(PAD_SECONDS * SAMPLE_RATE)
        width = -(-length // step) * step
        padded = torch.nn.functional.pad(waveform, (0, width - length))
        mask = torch.zeros(padded.shape, dtype=torch.long, device=padded.device)
        mask[..., :length] = 1
        logits = self.model(padded, attention_mask=mask).logits
        frames = output_frames(self.model.config, length)
        return SimpleNamespace(logits=logits[:, :frames])


def align_words(segments: list[Segment], audio: Audio, device: str) -> Transcription:
    """Word-align Korean segments with the WhisperX aligner."""

    import whisperx

    model, metadata = whisperx.load_align_model(language_code="ko", device=device)
    kind = cast("dict[str, object]", metadata).get("type")
    runner = (
        PaddedAligner(cast(Wav2Vec2ForCTC, model)) if kind == "huggingface" else model
    )
    try:
        aligned = whisperx.align(segments, runner, metadata, audio, device)
    finally:
        del model, runner
        release_torch_cache()
    return cast(Transcription, cast(object, aligned))
