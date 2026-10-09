"""Contract tests for pure Galpi worker behavior."""

import json
import logging
import unittest
from contextlib import redirect_stdout
from io import StringIO

from ..galpi_worker.artifacts import Segment, filter_segments, format_timestamp
from ..galpi_worker.core import (
    ASR_HOTWORDS_CHAR_BUDGET,
    SpeakerHint,
    build_asr_hotwords,
    parse_asr_context,
    should_filter_segment,
    validate_speaker_hint,
)
from ..galpi_worker.preparation import DownloadReporter
from ..galpi_worker.protocol import EventWriter
from ..galpi_worker.runtime import configure_warnings, select_torch_device
from .minutes_prompt_cases import GlossaryTests, MapReduceTests, ParticipantTests
from .refine_stream_cases import RefinementTests

__all__ = [
    "AsrContextTests",
    "GlossaryTests",
    "HallucinationFilterTests",
    "MapReduceTests",
    "ParticipantTests",
    "RefinementTests",
    "SpeakerHintTests",
]


class SpeakerHintTests(unittest.TestCase):
    def test_rejects_zero_exact_speakers(self) -> None:
        # Given
        hint = SpeakerHint(mode="exact", exact=0)

        # When / Then
        with self.assertRaisesRegex(ValueError, "greater than zero"):
            validate_speaker_hint(hint)

    def test_rejects_reversed_speaker_range(self) -> None:
        # Given
        hint = SpeakerHint(mode="range", minimum=4, maximum=2)

        # When / Then
        with self.assertRaisesRegex(ValueError, "minimum"):
            validate_speaker_hint(hint)


class DownloadReporterTests(unittest.TestCase):
    def reporter(self, buffer: StringIO) -> DownloadReporter:
        return DownloadReporter(EventWriter(stream=buffer), 10.0, 40.0)

    def test_maps_downloaded_bytes_onto_the_phase_band(self) -> None:
        # Given: two files whose sizes are known before any byte arrives
        buffer = StringIO()
        reporter = self.reporter(buffer)
        reporter.add_total(600)
        reporter.add_total(400)

        # When: half the total has landed
        reporter.add_progress(500)

        # Then: the phase percent sits halfway through the band
        event = json.loads(buffer.getvalue().strip())
        self.assertEqual(event["phase"], "models")
        self.assertEqual(event["percent"], 25.0)

    def test_throttles_so_a_fast_download_cannot_flood_the_stream(self) -> None:
        # Given
        buffer = StringIO()
        reporter = self.reporter(buffer)
        reporter.add_total(1000)

        # When: many chunks arrive back to back
        for _ in range(50):
            reporter.add_progress(10)

        # Then: only the first one is reported
        self.assertEqual(len(buffer.getvalue().strip().splitlines()), 1)

    def test_reports_nothing_before_a_size_is_known(self) -> None:
        # Given / When: bytes arrive before any bar declared its total
        buffer = StringIO()
        self.reporter(buffer).add_progress(10)

        # Then
        self.assertEqual(buffer.getvalue(), "")


class AsrContextTests(unittest.TestCase):
    def test_parses_the_host_biasing_lists(self) -> None:
        # Given
        payload: object = {
            "terms": [" 갈피 ", "화자분리", ""],
            "names": ["하빈", None, 3, "지우"],
            "aliases": ["프로님"],
        }

        # When
        terms, names, aliases = parse_asr_context(payload)

        # Then
        self.assertEqual(terms, ["갈피", "화자분리"])
        self.assertEqual(names, ["하빈", "지우"])
        self.assertEqual(aliases, ["프로님"])

    def test_accepts_missing_keys_as_empty_lists(self) -> None:
        # Given / When
        terms, names, aliases = parse_asr_context({})

        # Then
        self.assertEqual((terms, names, aliases), ([], [], []))

    def test_rejects_a_non_object_payload(self) -> None:
        # Given / When / Then
        with self.assertRaisesRegex(TypeError, "was not a JSON object"):
            _ = parse_asr_context(["갈피"])

    def test_rejects_a_non_array_entry(self) -> None:
        # Given / When / Then
        with self.assertRaisesRegex(TypeError, "terms was not a JSON array"):
            _ = parse_asr_context({"terms": "갈피"})

    def test_orders_glossary_terms_before_names_and_aliases(self) -> None:
        # Given / When
        hotwords = build_asr_hotwords(["갈피"], ["하빈", "지우"], ["프로님", "지우님"])

        # Then: rare domain words first, people next, spoken aliases last
        self.assertEqual(hotwords, "갈피, 하빈, 지우, 프로님, 지우님")

    def test_deduplicates_repeated_entries(self) -> None:
        # Given / When
        hotwords = build_asr_hotwords(["갈피"], ["갈피", "하빈"], ["하빈"])

        # Then
        self.assertEqual(hotwords, "갈피, 하빈")

    def test_drops_whole_entries_once_the_budget_runs_out(self) -> None:
        # Given: terms long enough that the third cannot fit
        long_terms = ["가" * 80, "나" * 80, "다" * 80]

        # When
        hotwords = build_asr_hotwords(long_terms, [], [])

        # Then: the first two fit (80 + 2 + 80 = 162); the third would exceed
        # the budget and is dropped whole rather than cut mid-word.
        self.assertEqual(hotwords, "가" * 80 + ", " + "나" * 80)
        self.assertLessEqual(len(hotwords), ASR_HOTWORDS_CHAR_BUDGET)

    def test_returns_nothing_without_any_entries(self) -> None:
        # Given / When / Then
        self.assertEqual(build_asr_hotwords([], [], []), "")


class HallucinationFilterTests(unittest.TestCase):
    def test_filters_known_subscription_phrase(self) -> None:
        # Given
        text = "시청해 주셔서 감사합니다. 구독과 좋아요 부탁드립니다."

        # When
        filtered = should_filter_segment(text)

        # Then
        self.assertTrue(filtered)

    def test_preserves_short_real_utterance(self) -> None:
        # Given
        text = "잠시만요"

        # When
        filtered = should_filter_segment(text)

        # Then
        self.assertFalse(filtered)

    def test_filters_a_repeated_phrase_no_single_token_dominates(self) -> None:
        # Given: the decoder stalled on a whole phrase, so every token stays
        # well under the single-token dominance bar
        text = "네 알겠습니다 " * 6

        # When
        filtered = should_filter_segment(text)

        # Then
        self.assertTrue(filtered)

    def test_preserves_natural_speech_that_reuses_common_words(self) -> None:
        # Given: ordinary meeting speech repeats fillers without looping
        text = (
            "네 좋습니다 그럼 그렇게 진행하시죠 확인 후에 다시 말씀드릴게요 감사합니다"
        )

        # When
        filtered = should_filter_segment(text)

        # Then
        self.assertFalse(filtered)

    def test_filters_repeated_name_loop_from_silence(self) -> None:
        # Given: Whisper repeating one name through a silent stretch
        text = "김관식, " * 56

        # When
        filtered = should_filter_segment(text)

        # Then
        self.assertTrue(filtered)

    def test_filters_repetition_loop_even_with_corrupted_copies(self) -> None:
        # Given: a dominant loop with a few corrupted variants still inside
        text = ", ".join(["김현현"] * 55 + ["김현현현"])

        # When
        filtered = should_filter_segment(text)

        # Then
        self.assertTrue(filtered)

    def test_keeps_brief_backchannel_repeats(self) -> None:
        # Given: a real speaker acknowledging a few times in one breath
        for text in ["네, 네, 네", "네 네 네 네 널"]:
            # When
            filtered = should_filter_segment(text)

            # Then
            self.assertFalse(filtered, text)

    def test_repetition_threshold_sits_at_six_dominant_tokens(self) -> None:
        # Given / When / Then: five repeats stay, six or more are a loop
        self.assertFalse(should_filter_segment("네, " * 5))
        self.assertTrue(should_filter_segment("네, " * 6))

    def test_keeps_diverse_normal_sentences(self) -> None:
        # Given: real meeting sentences never let one token dominate
        text = "그건 일차적으로는 하빈님이랑 제가 담당을 할게요. 최근에는 URL 쪽을 하빈님이 보고 있습니다."

        # When
        filtered = should_filter_segment(text)

        # Then
        self.assertFalse(filtered)

    def test_drops_only_low_confidence_or_implausibly_fast_tail_segments(self) -> None:
        # Given: speech in the first half, then a silence of two minutes after
        # which the decoder emits noise into the tail
        def segment(start: float, end: float, text: str, logprob: float) -> Segment:
            return Segment(start=start, end=end, text=text, avg_logprob=logprob)

        speech = segment(0.0, 60.0, "오늘 회의를 시작하겠습니다", -0.7)
        before_gap = segment(60.0, 70.0, "다음 안건으로 넘어가겠습니다", -0.1)
        unsure = segment(190.0, 192.0, "감사합니다", -0.71)
        too_fast = segment(193.0, 193.4, "가나다라마바사", -0.1)
        plausible = segment(194.0, 194.4, "네", -0.1)
        segments = [speech, before_gap, unsure, too_fast, plausible]

        # When
        kept, filtered = filter_segments(segments, audio_duration=120.0)

        # Then: before the gap nothing is judged by confidence; after it,
        # low confidence (< -0.7) or > 12 characters per second is noise
        self.assertEqual(kept, [speech, before_gap, plausible])
        self.assertEqual(filtered, [unsure, too_fast])

    def test_normalizes_rounded_srt_timestamp_carry(self) -> None:
        self.assertEqual(format_timestamp(59.9999), "00:01:00,000")

    def test_protocol_stream_isolated_from_dependency_stdout(self) -> None:
        protocol = StringIO()
        dependency_output = StringIO()
        writer = EventWriter(stream=protocol)

        with redirect_stdout(dependency_output):
            print("third-party diagnostic")
            writer.emit("phase", phase="transcribing", percent=1.0)

        self.assertNotIn("third-party", protocol.getvalue())
        self.assertIn('"type": "phase"', protocol.getvalue())

    def test_suppresses_lightning_checkpoint_migration_info(self) -> None:
        logger = logging.getLogger("lightning.pytorch.utilities.migration.utils")
        previous_level = logger.level
        try:
            configure_warnings()
            self.assertEqual(logger.level, logging.WARNING)
        finally:
            logger.setLevel(previous_level)

    def test_selects_mps_when_apple_gpu_is_available(self) -> None:
        self.assertEqual(select_torch_device(mps_available=True), "mps")
        self.assertEqual(select_torch_device(mps_available=False), "cpu")


if __name__ == "__main__":
    _ = unittest.main()
