"""Speaker labels for transcript segments, from diarization turns.

Pure: no Torch, pyannote, or WhisperX import, so the policy can be tested and
benchmarked against cached diarization output.
"""

from typing import Final, Literal, TypedDict

from .artifacts import Segment

# Which pyannote annotation the WhisperX path labels lines from.
TURN_VIEW: Final[Literal["regular", "exclusive"]] = "regular"


class Turn(TypedDict):
    start: float
    end: float
    speaker: str


def dominant_speaker(turns: list[Turn], start: float, end: float) -> str | None:
    """The speaker whose turns cover most of [start, end]; None without overlap."""

    totals: dict[str, float] = {}
    for turn in turns:
        overlap = min(end, turn["end"]) - max(start, turn["start"])
        if overlap > 0:
            totals[turn["speaker"]] = totals.get(turn["speaker"], 0.0) + overlap
    if not totals:
        return None
    return max(totals.items(), key=lambda item: item[1])[0]


def label_segments(segments: list[Segment], turns: list[Turn]) -> list[Segment]:
    """Give every segment the speaker covering most of it."""

    labelled: list[Segment] = []
    for segment in segments:
        speaker = dominant_speaker(turns, segment["start"], segment["end"])
        copy = Segment(**segment)
        if speaker is not None:
            copy["speaker"] = speaker
        labelled.append(copy)
    return labelled
