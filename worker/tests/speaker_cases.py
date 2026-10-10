"""Speaker attribution cases: diarization turns and line voices to line labels."""

import unittest

from ..galpi_worker.artifacts import Segment
from ..galpi_worker.speakers import SpeakerTurn, label_segments, relabel_by_voice

CENTROIDS = {"SPEAKER_00": [1.0, 0.0, 0.0], "SPEAKER_01": [0.0, 1.0, 0.0]}


def line(speaker: str, start: float = 0.0, end: float = 1.0) -> Segment:
    return Segment(start=start, end=end, text="네.", speaker=speaker)


class LabelSegmentTests(unittest.TestCase):
    def test_takes_the_speaker_covering_most_of_the_line(self) -> None:
        # Given a line that starts in one turn but mostly sits in the next
        turns = [
            SpeakerTurn(start=0.0, end=1.2, speaker="SPEAKER_00"),
            SpeakerTurn(start=1.2, end=5.0, speaker="SPEAKER_01"),
        ]
        segments = [Segment(start=0.8, end=3.0, text="그렇죠.")]

        labelled = label_segments(segments, turns)

        self.assertEqual(labelled[0].get("speaker"), "SPEAKER_01")
        self.assertNotIn("speaker", segments[0])

    def test_leaves_a_line_without_any_turn_unlabelled(self) -> None:
        turns = [SpeakerTurn(start=0.0, end=1.0, speaker="SPEAKER_00")]
        segments = [Segment(start=4.0, end=5.0, text="네.")]

        self.assertNotIn("speaker", label_segments(segments, turns)[0])


class VoiceRelabelTests(unittest.TestCase):
    def test_moves_a_line_whose_voice_clearly_matches_another_speaker(self) -> None:
        # Given a reply diarized as SPEAKER_00 but spoken in SPEAKER_01's voice
        segments = [line("SPEAKER_00")]

        relabelled = relabel_by_voice(segments, [[0.1, 0.9, 0.1]], CENTROIDS)

        self.assertEqual(relabelled[0].get("speaker"), "SPEAKER_01")
        self.assertEqual(segments[0].get("speaker"), "SPEAKER_00")

    def test_keeps_the_label_when_the_voice_is_ambiguous(self) -> None:
        # Given a voice only slightly closer to the other speaker (< margin)
        segments = [line("SPEAKER_00")]

        relabelled = relabel_by_voice(segments, [[0.70, 0.75, 0.0]], CENTROIDS)

        self.assertEqual(relabelled[0].get("speaker"), "SPEAKER_00")

    def test_keeps_lines_it_cannot_judge(self) -> None:
        # Given a line without a voice, an unlabelled line, and a label with
        # no centroid
        segments = [
            line("SPEAKER_00"),
            Segment(start=1.0, end=2.0, text="네."),
            line("SPEAKER_02"),
        ]
        voices: list[list[float] | None] = [None, [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]]

        relabelled = relabel_by_voice(segments, voices, CENTROIDS)

        self.assertEqual(
            [segment.get("speaker") for segment in relabelled],
            ["SPEAKER_00", None, "SPEAKER_02"],
        )
