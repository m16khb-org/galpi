"""OpenAI-compatible SSE transport and monotonic progress reporting."""

import json
import os
import time
import urllib.error
import urllib.request
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from typing import cast

from .minutes_pipeline import ChatMessage
from .protocol import EventWriter

BASE_URL_VARIABLE = "GALPI_ASSISTANT_BASE_URL"
TRANSPORT_VARIABLE = "GALPI_ASSISTANT_TRANSPORT"
EFFORT_VARIABLE = "GALPI_ASSISTANT_REASONING_EFFORT"
DEFAULT_BASE_URL = "https://api.z.ai/api/coding/paas/v4"
DEFAULT_MODEL = "glm-5.3-flash"
MAX_OUTPUT_TOKENS = 32768
GLM_MAX_OUTPUT_TOKENS = 131072
REASONING_EFFORTS = frozenset({"low", "medium", "high", "max"})
REQUEST_TIMEOUT_SECONDS = 600
REFINE_STREAM_START_PERCENT = 35.0
REFINE_STREAM_CEILING_PERCENT = 88.0
PROGRESS_EMIT_INTERVAL_SECONDS = 1.5
PROGRESS_EMIT_INTERVAL_CHARS = 4096
SSE_DONE_MARKER = "[DONE]"


def strip_document_fence(document: str) -> str:
    """Remove a code fence that wraps the whole document."""

    if not document.startswith("```"):
        return document
    lines = document.splitlines()
    if len(lines) < 2 or lines[-1].strip() != "```":
        return document
    return "\n".join(lines[1:-1]).strip()


@dataclass(frozen=True)
class StreamChunk:
    """One parsed SSE event: visible text, reasoning text, and completion state."""

    content: str | None
    reasoning: str | None
    finish_reason: str | None


def parse_sse_content(line: str) -> str | None:
    """Extract delta content from one SSE line; None for non-content lines."""

    return parse_sse_chunk(line).content


def parse_sse_chunk(line: str) -> StreamChunk:
    """Parse one SSE line into content, reasoning, and finish_reason.

    Raises RuntimeError when the stream carries an error payload instead of
    a completion chunk.
    """

    empty = StreamChunk(content=None, reasoning=None, finish_reason=None)
    if not line.startswith("data:"):
        return empty
    data = line[len("data:") :].strip()
    if not data or data == SSE_DONE_MARKER:
        return empty
    payload = json.loads(data)
    if not isinstance(payload, dict):
        raise TypeError("sse payload was not a JSON object")
    fields = cast(dict[str, object], payload)
    error = fields.get("error")
    if error is not None:
        detail = json.dumps(error, ensure_ascii=False)
        raise RuntimeError(f"assistant stream failed: {detail}")
    choices = fields.get("choices")
    if not isinstance(choices, list) or not choices:
        return empty
    first = cast(list[object], choices)[0]
    if not isinstance(first, dict):
        return empty
    entry = cast(dict[str, object], first)
    finish = entry.get("finish_reason")
    delta = entry.get("delta")
    finish_reason = finish if isinstance(finish, str) else None
    if not isinstance(delta, dict):
        return StreamChunk(content=None, reasoning=None, finish_reason=finish_reason)
    fields_delta = cast(dict[str, object], delta)
    content = fields_delta.get("content")
    reasoning = fields_delta.get("reasoning_content")
    return StreamChunk(
        content=content if isinstance(content, str) else None,
        reasoning=reasoning if isinstance(reasoning, str) else None,
        finish_reason=finish_reason,
    )


def streaming_percent(
    chars: int,
    expected: int,
    start: float = REFINE_STREAM_START_PERCENT,
    ceiling: float = REFINE_STREAM_CEILING_PERCENT,
) -> float:
    """Map accumulated characters onto a monotonic streaming progress band."""

    if expected <= 0:
        return ceiling
    fraction = min(1.0, chars / expected)
    return round(start + (ceiling - start) * fraction, 1)


def consume_assistant_stream(
    lines: Iterable[str],
    events: EventWriter,
    expected_chars: int,
    model: str,
    *,
    progress_start: float = REFINE_STREAM_START_PERCENT,
    progress_ceiling: float = REFINE_STREAM_CEILING_PERCENT,
    activity: str = "회의록 작성",
) -> str:
    """Consume SSE lines into a document, reporting reasoning and writing progress.

    Reasoning models emit `reasoning_content` before any visible text; while
    only reasoning has arrived the percent stays at the band start but the
    message carries the live reasoning length. An empty document is reported
    against the stream's finish reason so length exhaustion is actionable.
    """

    parts: list[str] = []
    written = 0
    finish_reason: str | None = None
    reasoning_chars = 0
    emitted_chars = 0
    emitted_reasoning = 0
    emitted_at = time.monotonic()
    for raw_line in lines:
        line = raw_line.strip()
        chunk = parse_sse_chunk(line)
        if chunk.finish_reason is not None:
            finish_reason = chunk.finish_reason
        if chunk.reasoning is None and chunk.content is None:
            continue
        if chunk.reasoning is not None:
            reasoning_chars += len(chunk.reasoning)
        if chunk.content is not None:
            parts.append(chunk.content)
            written += len(chunk.content)
        now = time.monotonic()
        due_by_chars = (
            written - emitted_chars >= PROGRESS_EMIT_INTERVAL_CHARS
            or reasoning_chars - emitted_reasoning >= PROGRESS_EMIT_INTERVAL_CHARS
        )
        if due_by_chars or now - emitted_at >= PROGRESS_EMIT_INTERVAL_SECONDS:
            if written > 0:
                events.emit(
                    "phase",
                    phase="refining",
                    percent=streaming_percent(
                        written, expected_chars, progress_start, progress_ceiling
                    ),
                    message=f"{model} 모델로 {activity} 중입니다. {written:,}자",
                )
            else:
                events.emit(
                    "phase",
                    phase="refining",
                    percent=progress_start,
                    message=f"{model} 모델이 {activity} 구조를 계획하는 중입니다. 추론 {reasoning_chars:,}자",
                )
            emitted_chars = written
            emitted_reasoning = reasoning_chars
            emitted_at = now
    document = "".join(parts).strip()
    if document:
        return strip_document_fence(document)
    if finish_reason == "length":
        raise RuntimeError(
            "응답이 출력 길이 한도(max_tokens)에 도달해 회의록 본문이 만들어지지 못했습니다."
            " 모델을 바꾸거나 다시 시도해 주세요."
        )
    if finish_reason == "content_filter":
        raise RuntimeError("증강 제공자가 콘텐츠 정책으로 응답을 차단했습니다.")
    detail = f" (finish_reason: {finish_reason})" if finish_reason else ""
    raise RuntimeError(f"assistant returned an empty message{detail}")


def is_default_glm(model: str, base_url: str) -> bool:
    """Whether this request targets a GLM model on the default z.ai endpoint."""

    return base_url == DEFAULT_BASE_URL and model.lower().startswith("glm")


def build_request_body(
    model: str,
    messages: Sequence[ChatMessage],
    base_url: str,
    effort: str | None,
) -> bytes:
    """Assemble the chat-completion request body for one refinement.

    Reasoning stays on for GLM: the z.ai budget (131072) is large enough for
    reasoning plus the document. `reasoning_effort` is included only when the
    user chose one, so other providers see a clean OpenAI-compatible body.
    """

    payload: dict[str, object] = {
        "model": model,
        "messages": messages,
        "stream": True,
        "temperature": 0.2,
        "max_tokens": GLM_MAX_OUTPUT_TOKENS
        if is_default_glm(model, base_url)
        else MAX_OUTPUT_TOKENS,
    }
    if effort is not None:
        payload["reasoning_effort"] = effort
    return json.dumps(payload, ensure_ascii=False).encode("utf-8")


def request_minutes(
    messages: Sequence[ChatMessage],
    model: str,
    api_key: str,
    events: EventWriter,
    expected_chars: int,
    *,
    progress_start: float = REFINE_STREAM_START_PERCENT,
    progress_ceiling: float = REFINE_STREAM_CEILING_PERCENT,
    activity: str = "회의록 작성",
) -> str:
    """Stream the chat completion and report writing progress as it arrives."""

    if os.environ.get(TRANSPORT_VARIABLE) == "responses":
        # Deferred: responses_stream imports this module's helpers at load time.
        from .responses_stream import request_via_responses

        return request_via_responses(
            messages,
            model,
            api_key,
            events,
            expected_chars,
            progress_start=progress_start,
            progress_ceiling=progress_ceiling,
            activity=activity,
        )
    base_url = os.environ.get(BASE_URL_VARIABLE, DEFAULT_BASE_URL).rstrip("/")
    effort = os.environ.get(EFFORT_VARIABLE, "").strip().lower()
    if effort not in REASONING_EFFORTS:
        effort = None
    body = build_request_body(model, messages, base_url, effort)
    request = urllib.request.Request(
        f"{base_url}/chat/completions",
        data=body,
        method="POST",
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
            "Accept": "text/event-stream",
        },
    )
    try:
        with urllib.request.urlopen(
            request, timeout=REQUEST_TIMEOUT_SECONDS
        ) as response:
            return consume_assistant_stream(
                (raw.decode("utf-8", errors="replace") for raw in response),
                events,
                expected_chars,
                model,
                progress_start=progress_start,
                progress_ceiling=progress_ceiling,
                activity=activity,
            )
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")[:500]
        raise RuntimeError(
            f"assistant request failed ({error.code}): {detail}"
        ) from error
    except urllib.error.URLError as error:
        raise RuntimeError(f"assistant request failed: {error.reason}") from error
