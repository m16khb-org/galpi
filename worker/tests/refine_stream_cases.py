"""SSE transport, request, and single-pass refinement contract cases."""

import json
import os
import tempfile
import time
import unittest
from io import StringIO
from pathlib import Path
from typing import TypedDict

from ..galpi_worker.assistant_stream import (
    build_request_body,
    consume_assistant_stream,
    is_default_glm,
    parse_sse_chunk,
    parse_sse_content,
    streaming_percent,
    strip_document_fence,
)
from ..galpi_worker.minutes_prompt import (
    NO_BACKGROUND,
    build_messages,
    transcript_date,
)
from ..galpi_worker.protocol import EventWriter


class StreamDelta(TypedDict, total=False):
    content: str
    reasoning_content: str


class StreamChoice(TypedDict, total=False):
    delta: StreamDelta
    finish_reason: str


class StreamPayload(TypedDict):
    choices: list[StreamChoice]


class RefinementTests(unittest.TestCase):
    def test_carries_background_into_the_prompt(self) -> None:
        # Given
        transcript = "[SPEAKER_00] (0s) 배포 일정을 정합시다."
        background = "제품: 갈피\n팀: 하빈(팀리더)"

        # When
        messages = build_messages(transcript, background, [], [])

        # Then
        self.assertEqual(messages[0]["role"], "system")
        self.assertIn("갈피", messages[1]["content"])
        self.assertIn("배포 일정을 정합시다", messages[1]["content"])

    def test_marks_missing_background_instead_of_leaving_it_blank(self) -> None:
        # Given / When
        messages = build_messages("[SPEAKER_00] (0s) 안녕하세요.", "   ", [], [])

        # Then
        self.assertIn(NO_BACKGROUND, messages[1]["content"])

    def test_rejects_empty_transcript_before_calling_the_assistant(self) -> None:
        # Given / When / Then
        with self.assertRaisesRegex(ValueError, "transcript is empty"):
            _ = build_messages("   \n", "제품: 갈피", [], [])

    def test_reads_delta_content_from_an_sse_line(self) -> None:
        # Given: one OpenAI-compatible streaming chunk
        line = 'data: {"choices": [{"delta": {"content": "## 결정사항"}}]}'

        # When / Then
        self.assertEqual(parse_sse_content(line), "## 결정사항")

    def test_ignores_sse_lines_without_delta_content(self) -> None:
        # Given: comments, keep-alives, role-only deltas, and the done marker
        lines = [
            ": keep-alive",
            'data: {"choices": [{"delta": {"role": "assistant"}}]}',
            "data: [DONE]",
            "",
        ]

        # When / Then
        self.assertEqual([parse_sse_content(line) for line in lines], [None] * 4)

    def test_raises_when_the_stream_carries_an_error(self) -> None:
        # Given
        line = 'data: {"error": {"message": "invalid api key"}}'

        # When / Then
        with self.assertRaisesRegex(RuntimeError, "invalid api key"):
            _ = parse_sse_content(line)

    def test_maps_accumulated_chars_onto_the_progress_band(self) -> None:
        # Given / When / Then: monotonically approaches the ceiling, never past
        self.assertEqual(streaming_percent(0, 4000), 35.0)
        self.assertLess(streaming_percent(2000, 4000), streaming_percent(4000, 4000))
        self.assertEqual(streaming_percent(4000, 4000), 88.0)
        self.assertEqual(streaming_percent(999999, 4000), 88.0)

    def test_maps_a_zero_expectation_straight_to_the_ceiling(self) -> None:
        self.assertEqual(streaming_percent(10, 0), 88.0)

    def test_parses_reasoning_and_finish_reason_from_a_chunk(self) -> None:
        # Given / When
        chunk = parse_sse_chunk(
            "data: "
            + json.dumps(
                {"choices": [{"delta": {"reasoning_content": "먼저"}}]},
                ensure_ascii=False,
            )
        )
        done = parse_sse_chunk(
            "data: "
            + json.dumps({"choices": [{"delta": {}, "finish_reason": "length"}]})
        )

        # Then
        self.assertEqual(
            (chunk.content, chunk.reasoning, chunk.finish_reason), (None, "먼저", None)
        )
        self.assertEqual(
            (done.content, done.reasoning, done.finish_reason), (None, None, "length")
        )

    def sse(self, payload: StreamPayload) -> str:
        return "data: " + json.dumps(payload, ensure_ascii=False)

    def test_stream_reports_reasoning_progress_before_content_arrives(self) -> None:
        # Given: a reasoning-only burst long enough to trip the char throttle
        buffer = StringIO()
        events = EventWriter(stream=buffer)
        lines = [
            self.sse({"choices": [{"delta": {"reasoning_content": "생" * 5000}}]}),
            "data: [DONE]",
        ]

        # When: the stream ends without any content
        with self.assertRaises(RuntimeError):
            _ = consume_assistant_stream(iter(lines), events, 4000, "glm-5.3")

        # Then: the reasoning length reached the progress channel
        self.assertIn("추론 5,000자", buffer.getvalue())

    def test_stream_returns_the_document_after_reasoning(self) -> None:
        # Given
        buffer = StringIO()
        events = EventWriter(stream=buffer)
        lines = [
            self.sse({"choices": [{"delta": {"reasoning_content": "계획"}}]}),
            self.sse({"choices": [{"delta": {"content": "# 회의록"}}]}),
            self.sse({"choices": [{"delta": {"content": " 본문"}}]}),
            self.sse({"choices": [{"delta": {}, "finish_reason": "stop"}]}),
            "data: [DONE]",
        ]

        # When
        document = consume_assistant_stream(iter(lines), events, 4000, "glm-5.3")

        # Then
        self.assertEqual(document, "# 회의록 본문")

    def test_stream_exhausted_by_reasoning_reports_an_actionable_error(self) -> None:
        # Given: reasoning consumed the budget; no content chunk ever arrived
        lines = [
            self.sse({"choices": [{"delta": {"reasoning_content": "생각"}}]}),
            self.sse({"choices": [{"delta": {}, "finish_reason": "length"}]}),
            "data: [DONE]",
        ]

        # When / Then
        with self.assertRaisesRegex(RuntimeError, "max_tokens"):
            _ = consume_assistant_stream(
                iter(lines), EventWriter(stream=StringIO()), 4000, "glm-5.3"
            )

    def test_stream_blocked_by_a_filter_names_the_filter(self) -> None:
        # Given
        lines = [
            self.sse({"choices": [{"delta": {}, "finish_reason": "content_filter"}]}),
            "data: [DONE]",
        ]

        # When / Then
        with self.assertRaisesRegex(RuntimeError, "차단"):
            _ = consume_assistant_stream(
                iter(lines), EventWriter(stream=StringIO()), 4000, "glm-5.3"
            )

    def test_glm_budget_applies_only_on_the_default_zai_endpoint(self) -> None:
        # Given / When / Then
        self.assertTrue(
            is_default_glm("glm-5.3", "https://api.z.ai/api/coding/paas/v4")
        )
        self.assertFalse(is_default_glm("glm-5.3", "https://openrouter.ai/api/v1"))
        self.assertFalse(
            is_default_glm("claude-sonnet-4", "https://api.z.ai/api/coding/paas/v4")
        )

    def test_request_body_carries_effort_and_glm_budget(self) -> None:
        # Given
        messages = [{"role": "user", "content": "안녕"}]

        # When
        zai = json.loads(
            build_request_body(
                "glm-5.3", messages, "https://api.z.ai/api/coding/paas/v4", "max"
            )
        )
        other = json.loads(
            build_request_body(
                "glm-5.3", messages, "https://openrouter.ai/api/v1", None
            )
        )

        # Then: GLM on z.ai gets the large reasoning budget and the effort
        self.assertEqual(zai["max_tokens"], 131072)
        self.assertEqual(zai["reasoning_effort"], "max")
        # Other endpoints keep a clean body without the effort parameter
        self.assertEqual(other["max_tokens"], 32768)
        self.assertNotIn("reasoning_effort", other)
        self.assertEqual(other["model"], "glm-5.3")

    def test_unwraps_a_document_wrapped_in_one_code_fence(self) -> None:
        # Given
        document = "```markdown\n# 회의록\n\n## TL;DR\n- 결론\n```"

        # When / Then
        self.assertEqual(strip_document_fence(document), "# 회의록\n\n## TL;DR\n- 결론")

    def test_keeps_inner_code_fences_of_a_plain_document(self) -> None:
        # Given
        document = "# 회의록\n\n```text\n샘플\n```"

        # When / Then
        self.assertEqual(strip_document_fence(document), document)

    def test_carries_the_meeting_date_into_the_prompt(self) -> None:
        # Given / When
        messages = build_messages("[SPEAKER_00] (0s) 안녕.", "", [], [], "2026-01-15")

        # Then: the host-derived date reaches the user message as an anchor
        self.assertIn("2026-01-15", messages[1]["content"])

    def test_omits_the_date_line_without_a_meeting_date(self) -> None:
        # Given / When
        messages = build_messages("[SPEAKER_00] (0s) 안녕.", "", [], [])

        # Then: the message pair is unchanged when no date is supplied
        self.assertEqual(len(messages), 2)
        self.assertEqual(messages[1]["role"], "user")
        self.assertNotIn("회의일:", messages[1]["content"])

    def testtranscript_date_reads_the_file_mtime(self) -> None:
        # Given: a transcript file stamped at a known instant
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "meeting_화자별.txt"
            path.write_text("[SPEAKER_00] (0s) 안녕.", encoding="utf-8")
            stamp = time.mktime((2026, 1, 15, 9, 0, 0, 0, 0, -1))
            os.utime(path, (stamp, stamp))

            # When / Then
            self.assertEqual(transcript_date(path), "2026-01-15")

    def testtranscript_date_is_absent_for_a_missing_file(self) -> None:
        # Given / When / Then
        self.assertIsNone(transcript_date(Path("/nonexistent/meeting.txt")))


if __name__ == "__main__":
    _ = unittest.main()
