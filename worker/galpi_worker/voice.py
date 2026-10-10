"""Speaker embedding of each transcript line, for voice-based relabelling.

Heavy imports (numpy, torch) stay inside the function so the module can be
imported where only the pure policy in `speakers` is needed.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Protocol, cast

from .artifacts import Segment
from .protocol import EventWriter
from .runtime import TorchDevice, needs_cpu_fallback
from .speakers import relabel_by_voice

if TYPE_CHECKING:
    from collections.abc import Callable

    import torch

SAMPLE_RATE = 16000
# The embedding model needs about half a second of audio; shorter lines are
# read from their start onward up to this length.
MIN_VOICE_SECONDS = 0.5
# Ten seconds of a line identify its speaker as well as the whole line.
MAX_VOICE_SECONDS = 10.0
WIDTH_SECONDS = (1.0, 2.0, 4.0, 7.0, MAX_VOICE_SECONDS)
BATCH_SIZE = 32


class SpeakerEmbedding(Protocol):
    def __call__(self, waveforms: torch.Tensor) -> object: ...
    def to(self, device: torch.device) -> object: ...


def line_voices(
    embedding: SpeakerEmbedding,
    audio: object,
    segments: list[Segment],
    device: TorchDevice,
) -> list[list[float] | None]:
    """One embedding per segment; None where the model could not embed it.

    Lines longer than `MAX_VOICE_SECONDS` are read from their middle. Lines
    are batched in a few fixed widths so an accelerator compiles a handful of
    input shapes instead of one per line. A shorter line is repeated to fill
    its width: the model pools frame statistics, which repetition keeps,
    whereas zero padding or a pooling mask shifted short-line voices far from
    their unpadded embedding (cosine about 0.6).
    """

    import numpy as np
    import torch

    # WhisperX hands over a 1-D array, pyannote's waveform a (1, N) tensor.
    samples = np.asarray(audio, dtype=np.float32).reshape(-1)
    total = len(samples)
    shortest = int(MIN_VOICE_SECONDS * SAMPLE_RATE)
    longest = int(MAX_VOICE_SECONDS * SAMPLE_RATE)
    widths = [int(seconds * SAMPLE_RATE) for seconds in WIDTH_SECONDS]
    windows: list[tuple[int, int]] = []
    for segment in segments:
        start = min(max(0, int(segment["start"] * SAMPLE_RATE)), total)
        end = min(max(int(segment["end"] * SAMPLE_RATE), start + shortest), total)
        if end - start > longest:
            start += (end - start - longest) // 2
            end = start + longest
        windows.append((start, end))
    buckets: dict[int, list[int]] = {}
    for index, (start, end) in enumerate(windows):
        if end > start:
            width = next(w for w in widths if w >= end - start)
            buckets.setdefault(width, []).append(index)
    voices: list[list[float] | None] = [None] * len(windows)
    target = torch.device(device)
    # torch.from_numpy is annotated with a bare ndarray upstream.
    from_numpy = cast("Callable[[object], torch.Tensor]", torch.from_numpy)
    for width, members in sorted(buckets.items()):
        for first in range(0, len(members), BATCH_SIZE):
            batch = members[first : first + BATCH_SIZE]
            waveforms = np.zeros((len(batch), 1, width), dtype=np.float32)
            for row, index in enumerate(batch):
                start, end = windows[index]
                waveforms[row, 0] = np.resize(samples[start:end], width)
            with torch.inference_mode():
                result = embedding(from_numpy(waveforms).to(target))
            vectors = np.asarray(result, dtype=np.float64)
            for row, index in enumerate(batch):
                vector = vectors[row]
                if bool(np.isfinite(vector).all()):
                    voices[index] = [float(value) for value in vector]
    return voices


def relabel_lines(
    embedding: SpeakerEmbedding,
    audio: object,
    segments: list[Segment],
    centroids: dict[str, list[float]],
    device: TorchDevice,
    events: EventWriter,
) -> list[Segment]:
    """Relabel lines by their own voice, retrying once on CPU."""

    if not centroids:
        return segments
    try:
        voices = line_voices(embedding, audio, segments, device)
    except Exception:
        if not needs_cpu_fallback(device):
            raise
        events.log(f"{device.upper()} 목소리 확인에 실패해 CPU로 다시 시도합니다.")
        import torch

        embedding.to(torch.device("cpu"))
        voices = line_voices(embedding, audio, segments, "cpu")
    return relabel_by_voice(segments, voices, centroids)
