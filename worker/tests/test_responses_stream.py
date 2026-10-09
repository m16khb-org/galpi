"""Responses transport contract cases against loopback fake servers."""

import json
import os
import sys
import tempfile
import threading
import unittest
from collections.abc import Generator, Mapping
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from io import StringIO
from pathlib import Path
from typing import cast
from unittest.mock import patch

from ..galpi_worker import __main__ as worker_main
from ..galpi_worker import responses_stream
from ..galpi_worker.minutes_pipeline import (
    MAP_MAX_WORKERS,
    ChatMessage,
    MinutesContext,
    split_transcript,
)
from ..galpi_worker.protocol import EventWriter
from ..galpi_worker.refine import extract_chunk_notes, refine
from ..galpi_worker.responses_stream import (
    AssistantError,
    build_responses_body,
    consume_responses_stream,
    request_via_responses,
)

MESSAGES: list[ChatMessage] = [
    {"role": "system", "content": "규칙 하나"},
    {"role": "system", "content": "규칙 둘"},
    {"role": "user", "content": "전사본"},
]
LIMIT_BODY = {"error": {"code": "subscription_sharing_usage_limit_exceeded"}}


def sse(*events: Mapping[str, object]) -> list[str]:
    return [f"data: {json.dumps(event, ensure_ascii=False)}" for event in events]


def delta(text: str) -> dict[str, object]:
    return {"type": "response.output_text.delta", "delta": text}


COMPLETED: dict[str, object] = {"type": "response.completed"}


class Recorded:
    def __init__(self, headers: dict[str, str], body: bytes) -> None:
        self.headers = headers
        self.body = body


class FakeServer:
    status = 200
    payload = b""
    content_type = "text/event-stream"
    # Declare a chunk longer than the payload, then hang up mid-chunk.
    truncated = False

    def __init__(self) -> None:
        self.requests: list[Recorded] = []
        self.lock = threading.Lock()
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:
                length = int(self.headers.get("Content-Length", "0"))
                body = self.rfile.read(length)
                with outer.lock:
                    outer.requests.append(
                        Recorded({k.lower(): v for k, v in self.headers.items()}, body)
                    )
                self.send_response(outer.status)
                self.send_header("Content-Type", outer.content_type)
                if outer.truncated:
                    self.send_header("Transfer-Encoding", "chunked")
                    self.end_headers()
                    size = f"{len(outer.payload) + 64:x}\r\n".encode()
                    self.wfile.write(size + outer.payload)
                    return
                self.send_header("Content-Length", str(len(outer.payload)))
                self.end_headers()
                self.wfile.write(outer.payload)

            def log_message(self, format: str, *args: object) -> None:
                return

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"

    def respond_stream(self, text: str = "문서") -> None:
        self.status = 200
        self.content_type = "text/event-stream"
        self.payload = ("\n\n".join(sse(delta(text), COMPLETED)) + "\n\n").encode()

    def respond_truncated(self) -> None:
        self.status = 200
        self.content_type = "text/event-stream"
        self.payload = ("\n\n".join(sse(delta("문서"))) + "\n\n").encode()
        self.truncated = True

    def respond_json(self, status: int, body: object) -> None:
        self.status = status
        self.content_type = "application/json"
        self.payload = json.dumps(body).encode()


@contextmanager
def serving() -> Generator[FakeServer]:
    fake = FakeServer()
    thread = threading.Thread(target=fake.server.serve_forever, daemon=True)
    thread.start()
    try:
        yield fake
    finally:
        fake.server.shutdown()
        fake.server.server_close()
        thread.join()


@contextmanager
def responses_mode(fake: FakeServer) -> Generator[None]:
    env = {"GALPI_ASSISTANT_TRANSPORT": "responses", "GALPI_ASSISTANT_API_KEY": "tok"}
    with (
        patch.dict(os.environ, env),
        patch.object(responses_stream, "RESPONSES_BASE_URL", f"{fake.url}/v1"),
    ):
        yield


def quiet_events() -> EventWriter:
    return EventWriter(stream=StringIO())


def request_body(recorded: Recorded) -> dict[str, object]:
    return cast(dict[str, object], json.loads(recorded.body))


def long_transcript(lines: int = 105) -> str:
    return "\n".join(f"화자1: {'가' * 990}{index:03d}" for index in range(lines))


class BodyAndParserTests(unittest.TestCase):
    def test_body_merges_system_into_instructions(self) -> None:
        body = cast(dict[str, object], json.loads(build_responses_body(MESSAGES, "m")))
        self.assertEqual(body["instructions"], "규칙 하나\n\n규칙 둘")
        self.assertEqual(body["input"], [{"role": "user", "content": "전사본"}])
        self.assertIs(body["store"], False)
        self.assertIs(body["stream"], True)
        for key in ("max_output_tokens", "temperature", "reasoning"):
            self.assertNotIn(key, body)
        self.assertNotIn("previous_response_id", body)

    def test_stream_accumulates_deltas(self) -> None:
        lines = sse(delta("안"), delta("녕"), COMPLETED)
        self.assertEqual(
            consume_responses_stream(lines, quiet_events(), 10, "m"), "안녕"
        )

    def test_stream_without_completed_is_interrupted(self) -> None:
        with self.assertRaises(AssistantError) as raised:
            consume_responses_stream(sse(delta("문서")), quiet_events(), 10, "m")
        self.assertEqual(raised.exception.code, "CHATGPT_STREAM_INTERRUPTED")

    def test_failed_event_maps_usage_codes(self) -> None:
        for code, expected in (
            (
                "subscription_sharing_usage_limit_exceeded",
                "CHATGPT_USAGE_LIMIT_EXCEEDED",
            ),
            ("subscription_sharing_usage_unavailable", "CHATGPT_USAGE_UNAVAILABLE"),
        ):
            failed = {"type": "response.failed", "response": {"error": {"code": code}}}
            with self.assertRaises(AssistantError) as raised:
                consume_responses_stream(sse(failed), quiet_events(), 10, "m")
            self.assertEqual(raised.exception.code, expected)

    def test_incomplete_event(self) -> None:
        with self.assertRaises(AssistantError) as raised:
            consume_responses_stream(
                sse(delta("x"), {"type": "response.incomplete"}), quiet_events(), 1, "m"
            )
        self.assertEqual(raised.exception.code, "CHATGPT_RESPONSE_INCOMPLETE")


class HttpTests(unittest.TestCase):
    def test_http_errors_map_once_without_retry(self) -> None:
        cases: list[tuple[int, object, str]] = [
            (401, {"error": {"code": "x"}}, "CHATGPT_AUTH_REJECTED"),
            (403, {"error": {"code": "x"}}, "CHATGPT_ACCESS_FORBIDDEN"),
            (429, LIMIT_BODY, "CHATGPT_USAGE_LIMIT_EXCEEDED"),
            (503, {"detail": "unavailable"}, "CHATGPT_USAGE_UNAVAILABLE"),
            (
                401,
                {"detail": "subscription_sharing_invalid_user"},
                "CHATGPT_AUTH_REJECTED",
            ),
            (400, {"detail": "bad"}, "CHATGPT_UNSUPPORTED_REQUEST"),
            (500, {"error": {"code": "user_unavailable"}}, "CHATGPT_UNAVAILABLE"),
        ]
        for status, body, expected in cases:
            with self.subTest(status=status, expected=expected), serving() as fake:
                fake.respond_json(status, body)
                with responses_mode(fake), self.assertRaises(AssistantError) as raised:
                    request_via_responses(
                        MESSAGES, "m", "secret-token", quiet_events(), 10
                    )
                self.assertEqual(raised.exception.code, expected)
                self.assertEqual(len(fake.requests), 1)
                self.assertNotIn("secret-token", str(raised.exception))

    def test_request_headers_and_success(self) -> None:
        with serving() as fake:
            fake.respond_stream("본문")
            with responses_mode(fake):
                result = request_via_responses(MESSAGES, "m", "tok", quiet_events(), 10)
        self.assertEqual(result, "본문")
        headers = fake.requests[0].headers
        self.assertEqual(headers["authorization"], "Bearer tok")
        self.assertEqual(headers["user-agent"], "galpi-worker")

    def test_connection_cut_mid_chunk_is_interrupted(self) -> None:
        with serving() as fake:
            fake.respond_truncated()
            with responses_mode(fake), self.assertRaises(AssistantError) as raised:
                request_via_responses(MESSAGES, "m", "tok", quiet_events(), 10)
        self.assertEqual(raised.exception.code, "CHATGPT_STREAM_INTERRUPTED")
        self.assertEqual(len(fake.requests), 1)


class RefineTests(unittest.TestCase):
    def test_single_pass_writes_output_and_event(self) -> None:
        with serving() as fake, tempfile.TemporaryDirectory() as directory:
            fake.respond_stream("# 회의록")
            root = Path(directory)
            transcript = root / "t.txt"
            transcript.write_text("화자1: 안녕하세요", encoding="utf-8")
            output = root / "지정.md"
            sink = StringIO()
            with responses_mode(fake):
                refine(
                    transcript,
                    output,
                    None,
                    None,
                    None,
                    "m",
                    EventWriter(stream=sink),
                    meeting_date="2026-01-01",
                )
            self.assertEqual(output.read_text(encoding="utf-8").strip(), "# 회의록")
            types = [json.loads(line)["type"] for line in sink.getvalue().splitlines()]
            self.assertIn("refined", types)
            self.assertEqual(len(fake.requests), 1)

    def test_map_reduce_requests_are_all_responses_shaped(self) -> None:
        with serving() as fake, tempfile.TemporaryDirectory() as directory:
            fake.respond_stream("노트")
            root = Path(directory)
            transcript = root / "t.txt"
            transcript.write_text(long_transcript(), encoding="utf-8")
            with responses_mode(fake):
                refine(
                    transcript,
                    root / "o.md",
                    None,
                    None,
                    None,
                    "m",
                    quiet_events(),
                    meeting_date="2026-01-01",
                )
            self.assertEqual(len(fake.requests), 8)
            for recorded in fake.requests:
                body = request_body(recorded)
                items = cast(list[dict[str, str]], body["input"])
                self.assertNotIn("system", [item["role"] for item in items])
                self.assertIn("instructions", body)
                self.assertNotIn("messages", body)

    def test_unset_transport_uses_chat_completions_path(self) -> None:
        with serving() as fake, tempfile.TemporaryDirectory() as directory:
            fake.respond_json(500, {})
            root = Path(directory)
            transcript = root / "t.txt"
            transcript.write_text("화자1: 안녕", encoding="utf-8")
            env = {"GALPI_ASSISTANT_API_KEY": "k", "GALPI_ASSISTANT_BASE_URL": fake.url}
            with (
                patch.dict(os.environ, env),
                patch.object(responses_stream, "request_via_responses") as responses,
                self.assertRaises(RuntimeError),
            ):
                os.environ.pop("GALPI_ASSISTANT_TRANSPORT", None)
                refine(
                    transcript,
                    root / "o.md",
                    None,
                    None,
                    None,
                    "m",
                    quiet_events(),
                    meeting_date="2026-01-01",
                )
            responses.assert_not_called()
            self.assertEqual(len(fake.requests), 1)


class MainTests(unittest.TestCase):
    def test_main_reports_assistant_error_as_event(self) -> None:
        with serving() as fake, tempfile.TemporaryDirectory() as directory:
            fake.respond_json(429, LIMIT_BODY)
            root = Path(directory)
            transcript = root / "t.txt"
            transcript.write_text("화자1: 안녕", encoding="utf-8")
            argv = [
                "galpi", "refine", "--transcript", str(transcript),
                "--output", str(root / "o.md"), "--model", "m",
                "--meeting-date", "2026-01-01",
            ]  # fmt: skip
            stdout, stderr = StringIO(), StringIO()
            with (
                responses_mode(fake),
                patch.object(sys, "argv", argv),
                patch.object(sys, "stdout", stdout),
                patch.object(sys, "stderr", stderr),
            ):
                code = worker_main.main()
        self.assertEqual(code, 1)
        events = [json.loads(line) for line in stdout.getvalue().splitlines()]
        self.assertEqual(events[-1]["type"], "error")
        self.assertEqual(events[-1]["code"], "CHATGPT_USAGE_LIMIT_EXCEEDED")
        self.assertNotIn("tok", stdout.getvalue())


class MapFailureTests(unittest.TestCase):
    CONTEXT = MinutesContext(
        background="없음", participants="", glossary="", meeting_date="2026-01-01"
    )

    def run_map(self) -> None:
        chunks = split_transcript(long_transcript())
        self.assertEqual(len(chunks), 7)
        extract_chunk_notes(chunks, self.CONTEXT, "m", "tok", quiet_events())

    def test_map_failure_stops_new_requests_in_responses_mode(self) -> None:
        with serving() as fake:
            fake.respond_json(429, LIMIT_BODY)
            with responses_mode(fake):
                for attempt in range(20):
                    fake.requests.clear()
                    with (
                        self.subTest(attempt=attempt),
                        self.assertRaises(AssistantError) as raised,
                    ):
                        self.run_map()
                    self.assertEqual(
                        raised.exception.code, "CHATGPT_USAGE_LIMIT_EXCEEDED"
                    )
                    self.assertLessEqual(len(fake.requests), MAP_MAX_WORKERS)

    def test_map_failure_stops_new_requests_in_api_key_mode(self) -> None:
        with serving() as fake:
            fake.respond_json(500, {"error": "boom"})
            env = {"GALPI_ASSISTANT_BASE_URL": fake.url}
            with patch.dict(os.environ, env):
                os.environ.pop("GALPI_ASSISTANT_TRANSPORT", None)
                for attempt in range(20):
                    fake.requests.clear()
                    with (
                        self.subTest(attempt=attempt),
                        self.assertRaises(RuntimeError) as raised,
                    ):
                        self.run_map()
                    self.assertIn("(500)", str(raised.exception))
                    self.assertLessEqual(len(fake.requests), MAP_MAX_WORKERS)


if __name__ == "__main__":
    unittest.main()
