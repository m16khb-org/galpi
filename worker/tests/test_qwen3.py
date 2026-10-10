"""Pure contract tests for the Qwen3 candidate pipeline."""

import unittest
from pathlib import Path

from ..galpi_worker.__main__ import build_parser
from ..galpi_worker.qwen3 import (
    BIAS_CONTEXT_CHAR_BUDGET,
    CHUNK_MAX_SECONDS,
    QWEN3_ENGINE_TAG,
    TimestampEntry,
    WordSpan,
    build_bias_context,
    build_word_spans,
    ffmpeg_decode_args,
    group_word_spans,
    matchable_chars,
    offset_entries,
    parse_silencedetect,
    plan_audio_chunks,
    read_word_checkpoint,
    word_entries,
    write_word_checkpoint,
)
from ..galpi_worker.speakers import SpeakerTurn


class Qwen3PipelineTests(unittest.TestCase):
    def test_chunks_stay_within_the_runtime_resplit_limit(self) -> None:
        # The runtime re-splits anything longer than 30s at an energy minimum
        # that can land mid-word, discarding Galpi's silence-aligned cut.
        self.assertLessEqual(CHUNK_MAX_SECONDS, 30.0)

    def test_normalizes_mlx_word_dicts_into_timestamps(self) -> None:
        words = [
            {"text": "안녕하세요", "start": 0.0, "end": 1.2},
            {"text": "반갑습니다", "start": 1.2, "end": 2.5},
        ]

        entries = word_entries(words)

        self.assertEqual(
            entries,
            [
                {"text": "안녕하세요", "start": 0.0, "end": 1.2},
                {"text": "반갑습니다", "start": 1.2, "end": 2.5},
            ],
        )

    def test_word_entries_tolerate_missing_timestamps(self) -> None:
        self.assertEqual(word_entries(None), [])
        self.assertEqual(word_entries([]), [])

    def test_maps_sentences_from_model_text_onto_chunk_times(self) -> None:
        # Given: the model text with spacing/punctuation plus bare word chunks
        text = "그건 일차적으로는 하빈님이랑 제가 담당할게요. 감사합니다."
        entries = [
            TimestampEntry(text="그건", start=0.0, end=0.5),
            TimestampEntry(text="일차적으로는", start=0.5, end=1.2),
            TimestampEntry(text="하빈님이랑", start=1.2, end=2.4),
            TimestampEntry(text="제가", start=2.4, end=3.2),
            TimestampEntry(text="담당할게요", start=3.2, end=4.0),
            TimestampEntry(text="감사합니다", start=5.0, end=6.0),
        ]

        segments = group_word_spans(build_word_spans(text, entries), [])

        # Then: sentences keep the model text verbatim; chunks only time them
        self.assertEqual(len(segments), 2)
        self.assertEqual(
            segments[0]["text"], "그건 일차적으로는 하빈님이랑 제가 담당할게요."
        )
        self.assertEqual(segments[0]["start"], 0.0)
        self.assertEqual(segments[0]["end"], 4.0)
        self.assertEqual(segments[1]["text"], "감사합니다.")
        self.assertEqual(segments[1]["start"], 5.0)

    def test_matchable_characters_follow_the_forced_aligner_rule(self) -> None:
        # Given
        text = "A+B_2026's·회의—끝"

        # When
        characters = matchable_chars(text)

        # Then: the aligner keeps Unicode letters/numbers and apostrophes only
        self.assertEqual(characters, list("AB2026's회의끝"))

    def test_sentence_split_needs_a_following_boundary(self) -> None:
        # Given: punctuation without a following space stays inside the sentence
        text = "3.14를 말하고 있습니다. 끝."
        entries = [
            TimestampEntry(text="3", start=0.0, end=0.4),
            TimestampEntry(text="14를", start=0.4, end=0.8),
            TimestampEntry(text="말하고", start=0.8, end=1.4),
            TimestampEntry(text="있습니다", start=1.4, end=2.0),
            TimestampEntry(text="끝", start=2.2, end=2.6),
        ]

        segments = group_word_spans(build_word_spans(text, entries), [])

        # Then
        self.assertEqual(len(segments), 2)
        self.assertEqual(segments[0]["text"], "3.14를 말하고 있습니다.")
        self.assertEqual(segments[1]["text"], "끝.")

    def test_splits_one_unpunctuated_stretch_at_the_speaker_change(self) -> None:
        # Given: a long run of speech with no terminal punctuation at all,
        # spoken by two people in turn
        spans = [
            WordSpan(text="그건 ", start=0.0, end=1.0),
            WordSpan(text="제가 ", start=1.0, end=2.0),
            WordSpan(text="맡을게요 ", start=2.0, end=3.0),
            WordSpan(text="네 ", start=3.0, end=4.0),
            WordSpan(text="부탁드립니다", start=4.0, end=5.0),
        ]
        turns = [
            SpeakerTurn(start=0.0, end=3.0, speaker="SPEAKER_00"),
            SpeakerTurn(start=3.0, end=5.0, speaker="SPEAKER_01"),
        ]

        # When
        segments = group_word_spans(spans, turns)

        # Then: the change of speaker ends the segment even without punctuation
        self.assertEqual(len(segments), 2)
        self.assertEqual(segments[0]["text"], "그건 제가 맡을게요")
        self.assertEqual(segments[0].get("speaker"), "SPEAKER_00")
        self.assertEqual(segments[1]["text"], "네 부탁드립니다")
        self.assertEqual(segments[1].get("speaker"), "SPEAKER_01")

    def test_splits_after_a_breath_long_pause(self) -> None:
        # Given: one speaker, one pause longer than a breath
        spans = [
            WordSpan(text="확인했습니다 ", start=0.0, end=1.0),
            WordSpan(text="다음", start=3.0, end=3.5),
        ]
        turns = [SpeakerTurn(start=0.0, end=4.0, speaker="SPEAKER_00")]

        # When
        segments = group_word_spans(spans, turns)

        # Then
        self.assertEqual(
            [segment["text"] for segment in segments], ["확인했습니다", "다음"]
        )

    def test_a_word_between_turns_takes_the_nearest_speaker(self) -> None:
        # Given: diarization trimmed the span, leaving the word uncovered
        spans = [WordSpan(text="네", start=4.3, end=4.4)]
        turns = [
            SpeakerTurn(start=0.0, end=4.2, speaker="SPEAKER_00"),
            SpeakerTurn(start=6.0, end=9.0, speaker="SPEAKER_01"),
        ]

        # When
        segments = group_word_spans(spans, turns)

        # Then: the closest turn owns it rather than UNKNOWN
        self.assertEqual(segments[0].get("speaker"), "SPEAKER_00")

    def test_word_spans_keep_text_the_aligner_never_reached(self) -> None:
        # Given: the aligner emitted fewer words than the model text carries
        text = "안녕하세요 반갑습니다"
        entries = [TimestampEntry(text="안녕하세요", start=0.0, end=1.0)]

        # When
        spans = build_word_spans(text, entries)

        # Then: no transcript text is silently dropped
        self.assertEqual("".join(span["text"] for span in spans), text)

    def test_checkpoint_round_trips_and_rejects_the_other_engine(self) -> None:
        import json
        import tempfile

        spans = [WordSpan(text="안녕하세요 ", start=0.0, end=1.0)]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "meeting.aligned.v2.json"

            # When
            write_word_checkpoint(path, spans)

            # Then
            self.assertEqual(read_word_checkpoint(path), spans)
            payload = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(payload["engine"], QWEN3_ENGINE_TAG)

            # And a WhisperX checkpoint is never read back as Qwen3 words
            path.write_text(
                json.dumps({"engine": "whisperx", "segments": []}), encoding="utf-8"
            )
            self.assertIsNone(read_word_checkpoint(path))

    def test_missing_checkpoint_reads_as_absent(self) -> None:
        self.assertIsNone(read_word_checkpoint(Path("/nonexistent/x.json")))

    def test_builds_bias_context_from_the_asr_context_file(self) -> None:
        import json
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ctx.json"
            path.write_text(
                json.dumps(
                    {
                        "terms": ["화자분리", "갈피"],
                        "names": ["하빈"],
                        "aliases": ["프로님"],
                    },
                    ensure_ascii=False,
                ),
                encoding="utf-8",
            )

            context = build_bias_context(path)

        self.assertIn("도메인 용어: 화자분리, 갈피", context)
        self.assertIn("참석자 이름: 하빈", context)
        self.assertIn("별칭: 프로님", context)

    def test_plan_chunks_cut_at_silence_and_tail(self) -> None:
        # Given: 120s audio with silences near the 25s marks
        midpoints = [24.0, 49.0, 74.0, 99.0]

        chunks = plan_audio_chunks(120.0, midpoints)

        # Then: cuts land on the latest in-window silence; tail fits one piece
        self.assertEqual(chunks[0], (0.0, 24.0))
        self.assertEqual(chunks[1], (24.0, 49.0))
        self.assertEqual(chunks[-1], (99.0, 120.0))
        import itertools

        for (_, end), (next_start, _) in itertools.pairwise(chunks):
            self.assertEqual(end, next_start)
        for start, end in chunks:
            self.assertLessEqual(end - start, CHUNK_MAX_SECONDS)

    def test_plan_chunks_hard_cut_without_silence(self) -> None:
        # Given: continuous audio with no detectable silence
        chunks = plan_audio_chunks(70.0, [])

        # Then: cuts fall back to the maximum length
        self.assertEqual(chunks[0][1] - chunks[0][0], CHUNK_MAX_SECONDS)
        self.assertEqual(chunks[-1], (60.0, 70.0))

    def test_plan_chunks_keep_short_audio_whole(self) -> None:
        self.assertEqual(plan_audio_chunks(28.0, [14.0]), [(0.0, 28.0)])
        self.assertEqual(plan_audio_chunks(0.0, []), [])

    def test_parse_silencedetect_extracts_midpoints(self) -> None:
        # Given
        text = (
            "[silencedetect @ 0x1] silence_start: 12.300\n"
            "[silencedetect @ 0x1] silence_end: 13.1 | silence_duration: 0.8\n"
            "[silencedetect @ 0x1] silence_start: not-a-number\n"
            "[silencedetect @ 0x1] silence_end: 50.0\n"
        )

        midpoints = parse_silencedetect(text)

        # Then: pairs resolve to midpoints; unparseable starts are dropped
        self.assertEqual(midpoints, [12.7])

    def test_offset_entries_shift_chunk_times(self) -> None:
        # Given
        entries = [
            TimestampEntry(text="안녕", start=0.0, end=1.0),
        ]

        # When
        shifted = offset_entries(entries, 295.0)

        # Then
        self.assertEqual(shifted[0]["start"], 295.0)
        self.assertEqual(shifted[0]["end"], 296.0)

    def test_ffmpeg_decode_args_normalize_any_container(self) -> None:
        # Given / When
        args = ffmpeg_decode_args(Path("in.m4a"), Path("out.wav"))

        # Then: overwrite, resample to 16 kHz mono, WAV output
        self.assertEqual(
            args,
            ["-y", "-i", "in.m4a", "-ar", "16000", "-ac", "1", "out.wav"],
        )

    def test_bias_context_is_capped_so_it_cannot_crowd_out_the_audio(self) -> None:
        import json
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ctx.json"
            path.write_text(
                json.dumps({"terms": [f"용어{index}" for index in range(400)]}),
                encoding="utf-8",
            )

            context = build_bias_context(path)

        self.assertEqual(len(context), BIAS_CONTEXT_CHAR_BUDGET)

    def test_empty_context_when_no_file(self) -> None:
        self.assertEqual(build_bias_context(None), "")
        self.assertEqual(build_bias_context(Path("/nonexistent/ctx.json")), "")


class EngineFlagTests(unittest.TestCase):
    def test_transcribe_accepts_engine_choices(self) -> None:
        parser = build_parser()

        args = parser.parse_args(
            [
                "transcribe",
                "--input",
                "a.wav",
                "--output",
                "out",
                "--engine",
                "qwen3",
            ]
        )

        self.assertEqual(args.engine, "qwen3")

    def test_prepare_defaults_to_whisperx_and_accepts_qwen3(self) -> None:
        parser = build_parser()

        default = parser.parse_args(
            ["prepare", "--manifest", "m.json", "--engine-bin", "bin"]
        )
        qwen3 = parser.parse_args(
            [
                "prepare",
                "--manifest",
                "m.json",
                "--engine-bin",
                "bin",
                "--engine",
                "qwen3",
            ]
        )

        self.assertEqual(default.engine, "whisperx")
        self.assertEqual(qwen3.engine, "qwen3")

    def test_rejects_unknown_engines(self) -> None:
        parser = build_parser()

        with self.assertRaises(SystemExit):
            parser.parse_args(
                [
                    "transcribe",
                    "--input",
                    "a.wav",
                    "--output",
                    "out",
                    "--engine",
                    "turbo",
                ]
            )


if __name__ == "__main__":
    unittest.main()
