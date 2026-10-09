"""Artifact filtering and atomic file publication."""

import json
import os
from itertools import pairwise
from pathlib import Path
from typing import NotRequired, TypedDict

from .core import should_filter_segment


class Segment(TypedDict):
    start: float
    end: float
    text: str
    speaker: NotRequired[str]
    avg_logprob: NotRequired[float]


class Transcription(TypedDict):
    segments: list[Segment]
    # Names the engine that produced the checkpoint. Absent on files written
    # before the tag existed, which the WhisperX path still accepts.
    engine: NotRequired[str]


def filter_segments(
    segments: list[Segment],
    audio_duration: float,
) -> tuple[list[Segment], list[Segment]]:
    tail_start: float | None = None
    for previous, current in pairwise(segments):
        silence = current["start"] - previous["end"]
        if previous["end"] >= audio_duration / 2 and silence >= 120:
            tail_start = current["start"]
            break

    kept: list[Segment] = []
    filtered: list[Segment] = []
    for segment in segments:
        text = segment["text"].strip()
        tail_noise = False
        if tail_start is not None and segment["start"] >= tail_start:
            duration = segment["end"] - segment["start"]
            if segment.get("avg_logprob", 0.0) < -0.7:
                tail_noise = True
            elif duration < 0.5:
                # Characters per second; only short tail segments need it.
                spoken = sum(character.isalnum() for character in text)
                rate = spoken / duration if duration > 0 else float("inf")
                tail_noise = rate > 12
        (filtered if should_filter_segment(text) or tail_noise else kept).append(
            segment
        )
    return kept, filtered


def write_json_atomic(path: Path, payload: Transcription) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(payload, ensure_ascii=False), encoding="utf-8")
    temporary.replace(path)


def write_outputs_atomic(
    srt_path: Path, text_path: Path, segments: list[Segment]
) -> None:
    srt_temporary = srt_path.with_suffix(".srt.tmp")
    text_temporary = text_path.with_suffix(".txt.tmp")
    srt_lines: list[str] = []
    text_lines: list[str] = []
    for index, segment in enumerate(segments, 1):
        speaker = segment.get("speaker", "UNKNOWN")
        text = segment["text"].strip()
        start = format_timestamp(segment["start"])
        end = format_timestamp(segment["end"])
        srt_lines.append(f"{index}\n{start} --> {end}\n[{speaker}] {text}\n")
        text_lines.append(f"[{speaker}] ({segment['start']:.0f}s) {text}")
    srt_temporary.write_text("\n".join(srt_lines), encoding="utf-8")
    text_temporary.write_text("\n".join(text_lines) + "\n", encoding="utf-8")
    os.replace(srt_temporary, srt_path)
    os.replace(text_temporary, text_path)


def format_timestamp(seconds: float) -> str:
    total_milliseconds = round(max(seconds, 0.0) * 1000)
    hours, remainder = divmod(total_milliseconds, 3_600_000)
    minutes, remainder = divmod(remainder, 60_000)
    whole_seconds, milliseconds = divmod(remainder, 1000)
    return f"{hours:02}:{minutes:02}:{whole_seconds:02},{milliseconds:03}"
