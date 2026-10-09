"""Qwen3 candidate transcription pipeline owned by Galpi.

Runs Qwen3-ASR-1.7B through the MLX runtime (Metal GPU, 8-bit weights)
with the native MLX forced aligner producing word timestamps in the same
pass, then diarizes on the shared pyannote community-1 model. Heavy imports
stay inside functions so the pure helpers remain testable without the ML
stack.
"""

from __future__ import annotations

import gc
import json
import os
import subprocess
import tempfile
import unicodedata
import wave
from bisect import bisect_left, bisect_right
from collections.abc import Callable
from pathlib import Path
from typing import TypedDict, cast

from .artifacts import (
    Segment,
    Transcription,
    filter_segments,
    write_json_atomic,
    write_outputs_atomic,
)
from .core import InvalidInput, SpeakerHint, parse_asr_context, validate_speaker_hint
from .preparation import (
    PYANNOTE_MODEL_ID,
    QWEN3_MLX_MODEL_DIR_NAME,
)
from .protocol import EventWriter
from .runtime import configure_warnings, ffmpeg_executable, select_torch_device

# The aligner emits one entry per word; those words regroup into segments that
# end at terminal punctuation, at a speaker change, after a breath-long pause,
# or once a group has run for one breath of speech.
SENTENCE_ENDINGS = (".", "!", "?", "…")
MAX_SENTENCE_SECONDS = 12.0
SPEAKER_GAP_SECONDS = 0.8
# Galpi cuts long meetings near real silences so words stay intact, then hands
# each cut to the runtime. mlx-qwen3-asr re-splits anything longer than 30s at
# an energy minimum that can land mid-word, so a Galpi chunk stays at or under
# that limit and its own silence-aligned boundary is the one that survives.
# `max_new_tokens` stays unset: the runtime derives a per-chunk budget from the
# chunk duration, which is a tighter bound than one fixed number.
CHUNK_TARGET_SECONDS = 25.0
CHUNK_MAX_SECONDS = 30.0
SILENCE_NOISE_FLOOR = "-35dB"
SILENCE_MIN_DURATION = 0.6
SAMPLE_RATE = 16_000
# The hotword slot is finite; a runaway roster would crowd out the audio.
BIAS_CONTEXT_CHAR_BUDGET = 500
# Written into the checkpoint so the two engines never read each other's work.
QWEN3_ENGINE_TAG = "qwen3"


class TimestampEntry(TypedDict):
    """One aligner chunk: a text span with start and end seconds."""

    text: str
    start: float
    end: float


class MlxWord(TypedDict):
    """One MLX forced-aligner word dict entry."""

    text: str
    start: float
    end: float


class WordSpan(TypedDict):
    """One aligner word carrying its slice of the model's punctuated text."""

    text: str
    start: float
    end: float


class SpeakerTurn(TypedDict):
    """One diarization turn: a speaker label holding a time span."""

    start: float
    end: float
    speaker: str


def transcribe_qwen3(
    audio_path: Path,
    output_dir: Path,
    speaker_hint: SpeakerHint,
    events: EventWriter,
    asr_context_path: Path | None = None,
) -> None:
    """Recognize, timestamp, diarize, filter, and publish artifacts."""

    validate_speaker_hint(speaker_hint)
    if not audio_path.is_file():
        raise InvalidInput(f"audio file not found: {audio_path}")
    output_dir.mkdir(parents=True, exist_ok=True)
    configure_warnings()

    base_name = audio_path.stem
    checkpoint_path = output_dir / f"{base_name}.aligned.v2.json"

    # The Qwen3 stack decodes audio through libsndfile, which rejects
    # compressed containers (m4a/AAC, mov, ...). Every input goes through the
    # bundled ffmpeg into 16 kHz mono WAV first, like the WhisperX path.
    events.emit(
        "phase",
        phase="transcribing",
        percent=5.0,
        message="오디오를 ffmpeg로 디코딩합니다.",
    )
    with tempfile.TemporaryDirectory(prefix="galpi-qwen3-") as work_dir:
        working_audio = Path(work_dir) / "audio.wav"
        decode_to_wav(audio_path, working_audio)
        duration = audio_duration(working_audio)

        spans = read_word_checkpoint(checkpoint_path)
        if spans is None:
            spans = recognize_words(working_audio, duration, asr_context_path, events)
            write_word_checkpoint(checkpoint_path, spans)
        else:
            events.emit(
                "phase",
                phase="transcribing",
                percent=100.0,
                message="기존 전사 체크포인트를 사용합니다.",
            )
        events.emit(
            "phase",
            phase="aligning",
            percent=100.0,
            message="Qwen3 정렬기로 문장 시간을 확정했습니다.",
        )

        events.emit(
            "phase",
            phase="diarizing",
            percent=10.0,
            message="화자를 분리합니다.",
        )
        import torch

        device = select_torch_device(mps_available=torch.backends.mps.is_available())
        turns = diarize(working_audio, speaker_hint, device, events)
        assigned = group_word_spans(spans, turns)
        events.emit(
            "phase",
            phase="diarizing",
            percent=100.0,
            message="화자분리가 완료되었습니다.",
        )

    kept, filtered = filter_segments(assigned, duration)

    events.emit(
        "phase",
        phase="writing",
        percent=30.0,
        message="환각 구간을 정리하고 결과를 씁니다.",
    )
    srt_path = output_dir / f"{base_name}.srt"
    text_path = output_dir / f"{base_name}_화자별.txt"
    write_outputs_atomic(srt_path, text_path, kept)
    events.emit(
        "phase", phase="writing", percent=100.0, message="결과 파일을 저장했습니다."
    )
    events.emit(
        "completed",
        srt=str(srt_path),
        txt=str(text_path),
        checkpoint=str(checkpoint_path),
        segments=len(kept),
        filtered=len(filtered),
    )


def recognize_words(
    working_audio: Path,
    duration: float,
    asr_context_path: Path | None,
    events: EventWriter,
) -> list[WordSpan]:
    """Run the MLX ASR pass and return every aligner word on the meeting clock."""

    events.emit(
        "phase",
        phase="transcribing",
        percent=10.0,
        message="한국어 음성을 Qwen3(MLX)로 전사합니다. (Metal GPU)",
    )
    from mlx_qwen3_asr import Session

    # The readiness gate guarantees the converted 8-bit weights exist in
    # the app cache; a missing directory means prepare was skipped.
    asr_dir = mlx_asr_model_dir()
    if not asr_dir.joinpath("weights.safetensors").is_file():
        raise RuntimeError(f"MLX Qwen3 모델이 준비되지 않았습니다: {asr_dir}")
    session = Session(model=str(asr_dir))
    context = build_bias_context(asr_context_path)
    samples = read_samples(working_audio)
    chunks = plan_audio_chunks(duration, detect_silences(working_audio))
    spans: list[WordSpan] = []
    for index, (start, end) in enumerate(chunks):
        # The runtime treats a bare array as 16 kHz mono, which is exactly what
        # the decode step produced, so a slice needs no file of its own.
        window = samples[int(start * SAMPLE_RATE) : int(end * SAMPLE_RATE)]
        result = session.transcribe(
            window,
            language="Korean",
            context=context,
            return_timestamps=True,
        )
        report_generation_limit(index, len(chunks), result, events)
        # TranscriptionResult.segments is annotated with bare dicts
        # upstream; word_entries normalizes the shape.
        raw_segments = result.segments  # pyright: ignore[reportUnknownMemberType, reportUnknownVariableType]
        entries = offset_entries(word_entries(cast("object", raw_segments)), start)
        spans.extend(build_word_spans(result.text, entries))
        events.emit(
            "phase",
            phase="transcribing",
            percent=10.0 + 80.0 * (index + 1) / len(chunks),
            message=f"전사 중… 구간 {index + 1}/{len(chunks)}",
        )
    del session
    gc.collect()
    release_mps_cache()
    events.emit(
        "phase",
        phase="transcribing",
        percent=100.0,
        message="전사가 완료되었습니다.",
    )
    return spans


def report_generation_limit(
    index: int,
    total: int,
    result: object,
    events: EventWriter,
) -> None:
    """Log a chunk that stopped for any reason other than finishing its text.

    `length` means the tail of the chunk was never emitted and `repetition`
    means the decoder looped, so both leave a visible hole in the transcript
    that the operator should be able to see in the log.
    """

    reason = getattr(result, "finish_reason", None)
    if reason in (None, "eos", "stop"):
        return
    events.log(f"구간 {index + 1}/{total} 생성이 '{reason}' 상태로 끝났습니다.")


def read_samples(audio_path: Path) -> object:
    """Read the decoded 16 kHz mono WAV as one float32 array."""

    import numpy as np

    with wave.open(str(audio_path), "rb") as handle:
        if handle.getsampwidth() != 2:
            raise RuntimeError("expected 16-bit PCM wav")
        raw = handle.readframes(handle.getnframes())
        channels = handle.getnchannels()
    samples = np.frombuffer(raw, dtype="<i2").astype(np.float32) / 32768.0
    if channels > 1:
        samples = samples.reshape(-1, channels).mean(axis=1)
    return samples


def read_word_checkpoint(checkpoint_path: Path) -> list[WordSpan] | None:
    """Reuse a previous ASR pass, but only one this engine wrote.

    Speaker hints change far more often than the audio does, so the aligner
    words are worth keeping across runs. The engine tag stops a WhisperX
    checkpoint from being read back as Qwen3 words and the reverse.
    """

    if not checkpoint_path.is_file():
        return None
    try:
        payload = json.loads(checkpoint_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    if not isinstance(payload, dict) or payload.get("engine") != QWEN3_ENGINE_TAG:
        return None
    raw = payload.get("segments")
    if not isinstance(raw, list):
        return None
    spans: list[WordSpan] = []
    for item in cast("list[object]", raw):
        if not isinstance(item, dict):
            return None
        entry = cast("dict[str, object]", item)
        try:
            spans.append(
                WordSpan(
                    text=str(entry["text"]),
                    start=float(cast("float", entry["start"])),
                    end=float(cast("float", entry["end"])),
                )
            )
        except (KeyError, TypeError, ValueError):
            return None
    return spans


def write_word_checkpoint(checkpoint_path: Path, spans: list[WordSpan]) -> None:
    """Publish the aligner words so a re-run can skip the ASR pass."""

    write_json_atomic(
        checkpoint_path,
        Transcription(
            engine=QWEN3_ENGINE_TAG,
            segments=[
                Segment(start=span["start"], end=span["end"], text=span["text"])
                for span in spans
            ],
        ),
    )


def ffmpeg_decode_args(source: Path, destination: Path) -> list[str]:
    """ffmpeg invocation that normalizes any container to 16 kHz mono WAV."""

    return [
        "-y",
        "-i",
        str(source),
        "-ar",
        "16000",
        "-ac",
        "1",
        str(destination),
    ]


def decode_to_wav(source: Path, destination: Path) -> None:
    """Decode any supported container through the bundled ffmpeg binary."""

    ffmpeg = ffmpeg_executable()
    completed = subprocess.run(
        [ffmpeg, *ffmpeg_decode_args(source, destination)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if completed.returncode != 0 or not destination.is_file():
        raise RuntimeError(
            f"ffmpeg failed to decode {source.name} (exit {completed.returncode})"
        )


def plan_audio_chunks(
    duration: float,
    silence_midpoints: list[float],
    target: float = CHUNK_TARGET_SECONDS,
    maximum: float = CHUNK_MAX_SECONDS,
) -> list[tuple[float, float]]:
    """Plan near-target chunks, preferring silence midpoints as cut points.

    Cutting at silence keeps words intact; a stretch with no usable silence
    falls back to a hard cut at the maximum length.
    """

    if duration <= 0:
        return []
    chunks: list[tuple[float, float]] = []
    start = 0.0
    while start < duration - 0.01:
        if duration - start <= maximum:
            # The remaining tail fits one chunk.
            chunks.append((start, duration))
            break
        window_start = start + target - (maximum - target)
        hard_end = start + maximum
        candidates = [
            midpoint
            for midpoint in silence_midpoints
            if window_start <= midpoint <= hard_end
        ]
        end = max(candidates) if candidates else hard_end
        chunks.append((start, end))
        start = end
    return chunks


def parse_silencedetect(stderr_text: str) -> list[float]:
    """Extract silence midpoints from ffmpeg silencedetect output."""

    start: float | None = None
    midpoints: list[float] = []
    for line in stderr_text.splitlines():
        marker = line[line.find("silence_start:") :]
        if marker.startswith("silence_start:"):
            try:
                start = float(marker[len("silence_start:") :].strip().split()[0])
            except (ValueError, IndexError):
                start = None
            continue
        marker = line[line.find("silence_end:") :]
        if marker.startswith("silence_end:") and start is not None:
            try:
                end = float(marker[len("silence_end:") :].strip().split()[0])
            except (ValueError, IndexError):
                start = None
                continue
            midpoints.append((start + end) / 2)
            start = None
    return midpoints


def detect_silences(audio: Path) -> list[float]:
    """Find silence midpoints via ffmpeg silencedetect."""

    ffmpeg = ffmpeg_executable()
    completed = subprocess.run(
        [
            ffmpeg,
            "-i",
            str(audio),
            "-af",
            f"silencedetect=noise={SILENCE_NOISE_FLOOR}:d={SILENCE_MIN_DURATION}",
            "-f",
            "null",
            "-",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        check=False,
        text=True,
    )
    return parse_silencedetect(completed.stderr)


def offset_entries(
    entries: list[TimestampEntry],
    offset: float,
) -> list[TimestampEntry]:
    """Shift chunk-local timestamps onto the full-meeting timeline."""

    return [
        TimestampEntry(
            text=entry["text"],
            start=entry["start"] + offset,
            end=entry["end"] + offset,
        )
        for entry in entries
    ]


def mlx_asr_model_dir() -> Path:
    """Resolve the converted 8-bit MLX ASR weights inside the app cache."""

    hf_home = os.environ.get("HF_HOME")
    cache_root = Path(hf_home).parent if hf_home else Path.home() / ".cache"
    return cache_root / "mlx" / QWEN3_MLX_MODEL_DIR_NAME


def word_entries(raw_words: object) -> list[TimestampEntry]:
    """Normalize MLX aligner word dicts into timestamp entries.

    The MLX aligner publishes plain dicts with text/start/end keys; None
    means the chunk produced no timestamps at all.
    """

    if not isinstance(raw_words, list):
        return []
    words = cast("list[MlxWord]", raw_words)
    return [
        TimestampEntry(
            text=str(word["text"]),
            start=float(word["start"]),
            end=float(word["end"]),
        )
        for word in words
    ]


def release_mps_cache() -> None:
    """Return accelerator memory to the system before diarization loads."""

    try:
        import mlx.core as mx

        # mlx moved the cache reset off the metal namespace in 0.29; the old
        # path still works but warns, so prefer the new one when present.
        clear_cache = getattr(mx, "clear_cache", None) or mx.metal.clear_cache
        clear_cache()
    except (AttributeError, ImportError, RuntimeError):
        pass
    try:
        import torch

        if torch.backends.mps.is_available():
            torch.mps.empty_cache()
    except (AttributeError, ImportError, RuntimeError):
        pass


def build_bias_context(asr_context_path: Path | None) -> str:
    """Turn the glossary/roster biasing file into a freeform Qwen3 hint."""

    if asr_context_path is None or not asr_context_path.is_file():
        return ""
    terms, names, aliases = parse_asr_context(
        json.loads(asr_context_path.read_text(encoding="utf-8"))
    )
    parts: list[str] = []
    if terms:
        parts.append("도메인 용어: " + ", ".join(terms))
    if names:
        parts.append("참석자 이름: " + ", ".join(names))
    if aliases:
        parts.append("별칭: " + ", ".join(aliases))
    return "\n".join(parts)[:BIAS_CONTEXT_CHAR_BUDGET]


def build_word_spans(
    transcription_text: str,
    entries: list[TimestampEntry],
) -> list[WordSpan]:
    """Lay the model text over the aligner words, one timed span per word.

    The full `text` carries the spacing and punctuation the aligner strips, so
    each span takes the raw slice of text its word consumed, together with any
    punctuation that trails it. Timing comes from the aligner entry.
    """

    text = transcription_text.strip()
    length = len(text)
    # Classify each distinct character once; a meeting reuses a small
    # alphabet, and the cursor loops below visit every character.
    classes = {character: is_matchable(character) for character in set(text)}
    matchable = [classes[character] for character in text]
    word_lengths: dict[str, int] = {}
    spans: list[WordSpan] = []
    cursor = 0
    for entry in entries:
        word = entry["text"]
        needed = word_lengths.get(word)
        if needed is None:
            needed = word_lengths[word] = len(matchable_chars(word))
        if needed == 0:
            continue
        span_start = cursor
        consumed = 0
        while cursor < length and consumed < needed:
            if matchable[cursor]:
                consumed += 1
            cursor += 1
        # Trailing punctuation and spacing belong to the word just consumed,
        # so a sentence-ending mark stays attached to its own word.
        while cursor < length and not matchable[cursor]:
            cursor += 1
        piece = text[span_start:cursor]
        if piece.strip():
            spans.append(WordSpan(text=piece, start=entry["start"], end=entry["end"]))
    if spans and cursor < len(text):
        # Text the aligner never reached still belongs in the transcript.
        remainder = text[cursor:]
        if remainder.strip():
            spans[-1] = WordSpan(
                text=spans[-1]["text"] + remainder,
                start=spans[-1]["start"],
                end=spans[-1]["end"],
            )
    return spans


class SpeakerIndex:
    """Turns sorted by start, so one word only compares the turns near it.

    Scanning every turn for every word made speaker assignment quadratic: a
    three-hour meeting is ~20k words against ~1k turns. The answer is exactly
    the linear scan's: the turn covering the most of the word (the earliest
    turn on a tie), else the nearest turn.
    """

    def __init__(self, turns: list[SpeakerTurn]) -> None:
        self.turns = turns
        order = sorted(range(len(turns)), key=lambda index: turns[index]["start"])
        self.order = order
        self.starts = [turns[index]["start"] for index in order]
        # Running maximum of `end` in start order: every turn before the first
        # index whose running end passes a word's start ends before the word.
        self.reach: list[float] = []
        furthest = float("-inf")
        for index in order:
            furthest = max(furthest, turns[index]["end"])
            self.reach.append(furthest)

    def speaker(self, span: WordSpan) -> str:
        turns = self.turns
        if not turns:
            return "UNKNOWN"
        span_start = span["start"]
        span_end = span["end"]
        best_overlap = 0.0
        best_index = -1
        first = bisect_right(self.reach, span_start)
        last = bisect_left(self.starts, span_end)
        for position in range(first, last):
            index = self.order[position]
            turn = turns[index]
            overlap = min(span_end, turn["end"]) - max(span_start, turn["start"])
            if overlap > best_overlap or (
                overlap == best_overlap and best_index > index and overlap > 0
            ):
                best_overlap = overlap
                best_index = index
        if best_index >= 0:
            return turns[best_index]["speaker"]
        # A word falling in a gap between turns still has an owner: diarization
        # simply trimmed the span. The closest turn is a better guess than UNKNOWN.
        nearest = min(
            turns,
            key=lambda turn: min(
                abs(turn["start"] - span_end), abs(span_start - turn["end"])
            ),
        )
        return nearest["speaker"]


def ends_sentence(piece: str) -> bool:
    """Report whether a word closes a sentence.

    A terminal mark only ends a sentence when whitespace follows it, the same
    boundary a reader sees. Without that rule the dot inside "3.14" would cut
    the number in half.
    """

    stripped = piece.rstrip()
    return stripped != piece and stripped.endswith(SENTENCE_ENDINGS)


def group_word_spans(
    spans: list[WordSpan],
    turns: list[SpeakerTurn],
) -> list[Segment]:
    """Merge timed words into speaker-labelled segments.

    A segment breaks at a terminal punctuation mark, at a speaker change, after
    a breath-long pause, or once it has run for `MAX_SENTENCE_SECONDS`. Breaking
    on the speaker is what keeps one long unpunctuated stretch from collapsing
    several people into whoever spoke longest.
    """

    segments: list[Segment] = []
    current: list[str] = []
    current_speaker = ""
    start = 0.0
    end = 0.0

    def flush() -> None:
        nonlocal current
        text = "".join(current).strip()
        if text:
            segments.append(
                Segment(start=start, end=end, text=text, speaker=current_speaker)
            )
        current = []

    index = SpeakerIndex(turns)
    for span in spans:
        speaker = index.speaker(span)
        if current:
            closed = ends_sentence(current[-1])
            if (
                closed
                or speaker != current_speaker
                or span["start"] - end >= SPEAKER_GAP_SECONDS
                or span["end"] - start > MAX_SENTENCE_SECONDS
            ):
                flush()
        if not current:
            start = span["start"]
            current_speaker = speaker
        current.append(span["text"])
        end = span["end"]
    flush()
    return segments


def is_matchable(character: str) -> bool:
    """Apply the exact character rule used by the MLX forced aligner."""

    return character == "'" or unicodedata.category(character)[0] in "LN"


def matchable_chars(text: str) -> list[str]:
    """The characters of `text` the MLX forced aligner counts."""

    return [character for character in text if is_matchable(character)]


def diarize(
    audio_path: Path,
    hint: SpeakerHint,
    device: str,
    events: EventWriter,
) -> list[SpeakerTurn]:
    """Run pyannote community-1 with one MPS-to-CPU retry."""

    import torch
    from pyannote.audio import Pipeline

    waveform, sample_rate = load_waveform(audio_path)
    audio_input = {"waveform": waveform, "sample_rate": sample_rate}

    def load(load_device: str) -> Pipeline:
        pipeline = Pipeline.from_pretrained(PYANNOTE_MODEL_ID)
        if load_device != "cpu":
            pipeline.to(torch.device(load_device))
        return pipeline

    try:
        pipeline = load(device)
    except Exception:
        if device != "mps":
            raise
        events.log("MPS 화자분리에 실패해 CPU로 다시 시도합니다.")
        pipeline = load("cpu")

    options: dict[str, int] = {}
    if hint.mode == "exact":
        options["num_speakers"] = hint.exact or 1
    elif hint.mode == "range":
        if hint.minimum is not None:
            options["min_speakers"] = hint.minimum
        if hint.maximum is not None:
            options["max_speakers"] = hint.maximum
    diarization_output = pipeline(audio_input, **options)
    # community-1 returns a DiarizeOutput dataclass; the *exclusive* variant
    # drops overlapping speech, which maps cleaner onto ASR segments.
    annotation = diarization_output.exclusive_speaker_diarization
    return [
        SpeakerTurn(
            start=turn.start,
            end=turn.end,
            speaker=str(speaker),
        )
        for turn, _track, speaker in annotation.itertracks(yield_label=True)
    ]


def load_waveform(audio_path: Path) -> tuple[object, int]:
    """Read mono 16 kHz samples for pyannote's waveform-dict input.

    The decoded working audio is always 16 kHz mono PCM produced by the
    bundled ffmpeg, so the stdlib ``wave`` module reads it without the
    torchaudio backend roulette (torchcodec builds break across torch
    versions and libsndfile rejects compressed containers anyway).
    """

    import numpy as np
    import torch

    with wave.open(str(audio_path), "rb") as handle:
        sample_rate = handle.getframerate()
        channels = handle.getnchannels()
        width = handle.getsampwidth()
        raw = handle.readframes(handle.getnframes())
    if width != 2:
        raise RuntimeError(f"expected 16-bit PCM wav, got {width * 8}-bit")
    samples = np.frombuffer(raw, dtype="<i2").astype(np.float32) / 32768.0
    if channels > 1:
        samples = samples.reshape(-1, channels).mean(axis=1)
    # torch.from_numpy is annotated with a bare ndarray upstream.
    from_numpy = cast("Callable[[object], torch.Tensor]", torch.from_numpy)
    return from_numpy(cast("object", samples)).unsqueeze(0), sample_rate


def audio_duration(audio_path: Path) -> float:
    """Read the audio length in seconds without loading samples."""

    with wave.open(str(audio_path), "rb") as handle:
        return handle.getnframes() / float(handle.getframerate())
