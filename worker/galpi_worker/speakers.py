"""Speaker labels for transcript segments, from diarization turns and voices.

Pure: no Torch, pyannote, WhisperX, or numpy import, so the policy can be
tested and benchmarked against cached diarization output.
"""

from math import sqrt
from typing import Final, TypedDict

from .artifacts import Segment

# A line moves to another speaker only when its voice is this much closer
# (cosine similarity) to that speaker's centroid than to its own.
VOICE_MARGIN: Final = 0.1


class SpeakerTurn(TypedDict):
    """One diarization turn: a speaker label holding a time span."""

    start: float
    end: float
    speaker: str


def dominant_speaker(turns: list[SpeakerTurn], start: float, end: float) -> str | None:
    """The speaker whose turns cover most of [start, end]; None without overlap."""

    totals: dict[str, float] = {}
    for turn in turns:
        overlap = min(end, turn["end"]) - max(start, turn["start"])
        if overlap > 0:
            totals[turn["speaker"]] = totals.get(turn["speaker"], 0.0) + overlap
    if not totals:
        return None
    return max(totals.items(), key=lambda item: item[1])[0]


def label_segments(segments: list[Segment], turns: list[SpeakerTurn]) -> list[Segment]:
    """Give every segment the speaker covering most of it."""

    labelled: list[Segment] = []
    for segment in segments:
        speaker = dominant_speaker(turns, segment["start"], segment["end"])
        copy = Segment(**segment)
        if speaker is not None:
            copy["speaker"] = speaker
        labelled.append(copy)
    return labelled


def _unit(vector: list[float]) -> list[float] | None:
    norm = sqrt(sum(value * value for value in vector))
    if norm == 0:
        return None
    return [value / norm for value in vector]


def relabel_by_voice(
    segments: list[Segment],
    voices: list[list[float] | None],
    centroids: dict[str, list[float]],
    margin: float = VOICE_MARGIN,
) -> list[Segment]:
    """Move a line to the speaker its own voice matches clearly better.

    Diarization labels each turn from embeddings of ten-second windows shared
    with neighbours, so a short reply or a stretch of a similar voice can land
    on the wrong cluster. Embedding the line by itself and comparing it with
    the diarization's speaker centroids catches those turns. Unlabelled lines,
    lines without a voice, and speakers without a centroid stay as they are.
    """

    units = {
        speaker: unit
        for speaker, vector in centroids.items()
        if (unit := _unit(vector)) is not None
    }
    relabelled: list[Segment] = []
    for segment, voice in zip(segments, voices, strict=True):
        copy = Segment(**segment)
        relabelled.append(copy)
        current = segment.get("speaker")
        line = _unit(voice) if voice is not None else None
        if current not in units or line is None:
            continue
        scores = {
            speaker: sum(a * b for a, b in zip(line, centroid, strict=True))
            for speaker, centroid in units.items()
        }
        best = max(scores.items(), key=lambda item: item[1])[0]
        if best != current and scores[best] - scores[current] >= margin:
            copy["speaker"] = best
    return relabelled
