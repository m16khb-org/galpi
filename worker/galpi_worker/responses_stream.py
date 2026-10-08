"""ChatGPT Responses API transport: request body, SSE parsing, stable errors."""

import http.client
import json
import time
import urllib.error
import urllib.request
from collections.abc import Iterable, Sequence
from typing import cast

from .assistant_stream import (
    PROGRESS_EMIT_INTERVAL_CHARS,
    PROGRESS_EMIT_INTERVAL_SECONDS,
    REFINE_STREAM_CEILING_PERCENT,
    REFINE_STREAM_START_PERCENT,
    REQUEST_TIMEOUT_SECONDS,
    streaming_percent,
    strip_document_fence,
)
from .minutes_pipeline import ChatMessage
from .protocol import EventWriter

RESPONSES_BASE_URL = "https://api.openai.com/v1"
USER_AGENT = "galpi-worker"
MESSAGES = {
    "CHATGPT_USAGE_LIMIT_EXCEEDED": (
        "ChatGPT 사용량 한도에 도달했습니다. ChatGPT 설정의 사용량에서 확인해 주세요. "
        "앱은 요청을 다시 보내지 않습니다."
    ),
    "CHATGPT_USAGE_UNAVAILABLE": (
        "ChatGPT 사용량을 확인하지 못했습니다. 잠시 뒤 다시 실행해 주세요."
    ),
    "CHATGPT_NOT_ELIGIBLE": "이 ChatGPT 계정은 이 기능을 사용할 수 없습니다.",
    "CHATGPT_UNSUPPORTED_REQUEST": (
        "ChatGPT가 이 요청 형식을 지원하지 않습니다. 모델을 바꿔 다시 시도해 주세요."
    ),
    "CHATGPT_AUTH_REJECTED": "ChatGPT 로그인이 거절되었습니다. 다시 로그인해 주세요.",
    "CHATGPT_ACCESS_FORBIDDEN": "ChatGPT가 이 요청의 접근을 허용하지 않았습니다.",
    "CHATGPT_UNAVAILABLE": (
        "ChatGPT 서비스를 사용할 수 없습니다. 잠시 뒤 다시 실행해 주세요."
    ),
    "CHATGPT_RESPONSE_INCOMPLETE": "ChatGPT 응답이 완료되지 않아 회의록이 만들어지지 못했습니다.",
    "CHATGPT_STREAM_INTERRUPTED": "ChatGPT 응답이 도중에 끊겼습니다. 다시 실행해 주세요.",
}


class AssistantError(RuntimeError):
    """A failure with a stable, host-visible error code."""

    def __init__(self, code: str) -> None:
        super().__init__(MESSAGES[code])
        self.code = code


def map_failure(status: int | None, code: str) -> str:
    """Map an HTTP status and/or server error code onto a stable worker code."""

    lowered = code.lower()
    if "usage_limit" in lowered or status == 429:
        return "CHATGPT_USAGE_LIMIT_EXCEEDED"
    if "invalid_user" in lowered or status == 401:
        return "CHATGPT_AUTH_REJECTED"
    if "not_eligible" in lowered or "ineligible" in lowered:
        return "CHATGPT_NOT_ELIGIBLE"
    if "user_unavailable" in lowered:
        return "CHATGPT_UNAVAILABLE"
    if "usage_unavailable" in lowered or status == 503:
        return "CHATGPT_USAGE_UNAVAILABLE"
    if status == 400:
        return "CHATGPT_UNSUPPORTED_REQUEST"
    if status == 403:
        return "CHATGPT_ACCESS_FORBIDDEN"
    return "CHATGPT_UNAVAILABLE"


def build_responses_body(messages: Sequence[ChatMessage], model: str) -> bytes:
    """Build the preview-safe body: no sampling, token, or reasoning fields."""

    instructions = "\n\n".join(m["content"] for m in messages if m["role"] == "system")
    body: dict[str, object] = {
        "model": model,
        "input": [
            {"role": m["role"], "content": m["content"]}
            for m in messages
            if m["role"] != "system"
        ],
        "store": False,
        "stream": True,
    }
    if instructions:
        body["instructions"] = instructions
    return json.dumps(body, ensure_ascii=False).encode("utf-8")


def _object(value: object) -> dict[str, object]:
    return cast(dict[str, object], value) if isinstance(value, dict) else {}


def _error_code(payload: object) -> str:
    """Read the code from `{"error": {"code"}}`, `{"response": {"error"}}` or detail."""

    root = _object(payload)
    nested = _object(root.get("response")).get("error", root.get("error"))
    code = _object(nested).get("code")
    if isinstance(code, str):
        return code
    detail = root.get("detail")
    if isinstance(detail, str):
        return detail
    return str(_object(detail).get("code", "")) if detail is not None else ""


def consume_responses_stream(
    lines: Iterable[str],
    events: EventWriter,
    expected_chars: int,
    model: str,
    *,
    progress_start: float = REFINE_STREAM_START_PERCENT,
    progress_ceiling: float = REFINE_STREAM_CEILING_PERCENT,
    activity: str = "회의록 작성",
) -> str:
    """Accumulate output text deltas; only `response.completed` is success."""

    parts: list[str] = []
    written = 0
    emitted_chars = 0
    emitted_at = time.monotonic()
    completed = False
    for raw_line in lines:
        line = raw_line.strip()
        if not line.startswith("data:"):
            continue
        try:
            event = _object(json.loads(line[5:].strip()))
        except ValueError:
            continue
        kind = event.get("type")
        if kind == "response.output_text.delta":
            delta = event.get("delta")
            if not isinstance(delta, str):
                continue
            parts.append(delta)
            written += len(delta)
            now = time.monotonic()
            if (
                written - emitted_chars >= PROGRESS_EMIT_INTERVAL_CHARS
                or now - emitted_at >= PROGRESS_EMIT_INTERVAL_SECONDS
            ):
                events.emit(
                    "phase",
                    phase="refining",
                    percent=streaming_percent(
                        written, expected_chars, progress_start, progress_ceiling
                    ),
                    message=f"{model} 모델로 {activity} 중입니다. {written:,}자",
                )
                emitted_chars = written
                emitted_at = now
        elif kind == "response.completed":
            completed = True
            break
        elif kind == "response.failed":
            raise AssistantError(map_failure(None, _error_code(event)))
        elif kind == "response.incomplete":
            raise AssistantError("CHATGPT_RESPONSE_INCOMPLETE")
        elif kind == "error":
            raise AssistantError("CHATGPT_STREAM_INTERRUPTED")
    if not completed:
        raise AssistantError("CHATGPT_STREAM_INTERRUPTED")
    document = "".join(parts).strip()
    if not document:
        raise RuntimeError("assistant returned an empty message")
    return strip_document_fence(document)


def request_via_responses(
    messages: Sequence[ChatMessage],
    model: str,
    access_token: str,
    events: EventWriter,
    expected_chars: int,
    *,
    progress_start: float = REFINE_STREAM_START_PERCENT,
    progress_ceiling: float = REFINE_STREAM_CEILING_PERCENT,
    activity: str = "회의록 작성",
) -> str:
    """POST one streaming Responses request. Never retries."""

    request = urllib.request.Request(
        f"{RESPONSES_BASE_URL}/responses",
        data=build_responses_body(messages, model),
        method="POST",
        headers={
            "Authorization": f"Bearer {access_token}",
            "Content-Type": "application/json",
            "Accept": "text/event-stream",
            "User-Agent": USER_AGENT,
        },
    )
    try:
        with urllib.request.urlopen(
            request, timeout=REQUEST_TIMEOUT_SECONDS
        ) as response:
            return consume_responses_stream(
                (raw.decode("utf-8", errors="replace") for raw in response),
                events,
                expected_chars,
                model,
                progress_start=progress_start,
                progress_ceiling=progress_ceiling,
                activity=activity,
            )
    except urllib.error.HTTPError as error:
        try:
            code = _error_code(json.loads(error.read().decode("utf-8", "replace")))
        except ValueError:
            code = ""
        finally:
            error.close()
        request_id = error.headers.get("x-request-id") or error.headers.get(
            "openai-request-id"
        )
        events.log(f"ChatGPT 요청 실패 (HTTP {error.code}, 요청 ID {request_id})")
        raise AssistantError(map_failure(error.code, code)) from error
    except urllib.error.URLError as error:
        raise AssistantError("CHATGPT_UNAVAILABLE") from error
    except OSError as error:
        raise AssistantError("CHATGPT_STREAM_INTERRUPTED") from error
    except http.client.HTTPException as error:
        # A connection cut mid-chunk raises IncompleteRead, which is not an OSError.
        raise AssistantError("CHATGPT_STREAM_INTERRUPTED") from error
