import unittest
from typing import cast

from ..galpi_worker.mlx_asr import (
    SAMPLE_RATE,
    MlxResult,
    MlxWhisper,
    VadChunk,
    chunk_segment,
    decode_chunks,
)


class ScriptedDecoder:
    """Returns one scripted result per call and records the audio it got."""

    def __init__(self, results: list[MlxResult]) -> None:
        self.results = results
        self.lengths: list[int] = []

    def transcribe(self, audio: object, **_options: object) -> dict[str, object]:
        self.lengths.append(len(cast(list[float], audio)))
        return cast(dict[str, object], self.results.pop(0))


class ChunkSegmentTests(unittest.TestCase):
    def test_joins_decoder_pieces_into_one_segment_with_chunk_timing(self) -> None:
        # Given: the decoder split one VAD chunk into two pieces
        result = MlxResult(
            segments=[
                {"text": " 오늘 회의를", "avg_logprob": -0.2},
                {"text": " 시작합니다.", "avg_logprob": -0.4},
            ]
        )

        # When
        segment = chunk_segment(12.3456, 40.1, result)

        # Then: one WhisperX-shaped segment carries the chunk's own timing
        assert segment is not None
        self.assertEqual(
            (segment["start"], segment["end"], segment["text"]),
            (12.346, 40.1, "오늘 회의를 시작합니다."),
        )
        self.assertAlmostEqual(segment.get("avg_logprob", 0.0), -0.3)

    def test_a_chunk_that_decoded_to_nothing_yields_no_segment(self) -> None:
        self.assertIsNone(chunk_segment(0.0, 5.0, MlxResult(segments=[{"text": "  "}])))
        self.assertIsNone(chunk_segment(0.0, 5.0, MlxResult()))


class DecodeChunksTests(unittest.TestCase):
    def test_decodes_each_chunk_slice_in_order_and_reports_progress(self) -> None:
        # Given: three VAD chunks over ten seconds of audio, the middle one silent
        audio = [0.0] * (10 * SAMPLE_RATE)
        chunks = [
            VadChunk(start=0.0, end=2.0),
            VadChunk(start=3.0, end=4.5),
            VadChunk(start=5.0, end=10.0),
        ]
        decoder = ScriptedDecoder(
            [
                MlxResult(segments=[{"text": "첫 문장"}]),
                MlxResult(segments=[]),
                MlxResult(text="마지막 문장"),
            ]
        )
        progress: list[float] = []

        # When
        result = decode_chunks(
            audio,
            chunks,
            cast(MlxWhisper, decoder),
            initial_prompt=None,
            progress=progress.append,
        )

        # Then: each decoder call saw exactly its chunk, the silent chunk is
        # dropped, and progress climbs to 100 once per chunk
        self.assertEqual(
            decoder.lengths, [2 * SAMPLE_RATE, int(1.5 * SAMPLE_RATE), 5 * SAMPLE_RATE]
        )
        self.assertEqual(
            [(s["start"], s["end"], s["text"]) for s in result["segments"]],
            [(0.0, 2.0, "첫 문장"), (5.0, 10.0, "마지막 문장")],
        )
        self.assertEqual(result.get("language"), "ko")
        self.assertEqual([round(p, 3) for p in progress], [33.333, 66.667, 100.0])


if __name__ == "__main__":
    unittest.main()
