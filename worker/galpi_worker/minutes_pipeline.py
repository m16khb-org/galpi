"""Pure long-meeting routing, chunking, and map/reduce prompt builders."""

from collections.abc import Sequence
from dataclasses import dataclass
from typing import Final, Literal, TypedDict

SINGLE_PASS_CHAR_LIMIT: Final = 48_000
CHUNK_CHAR_BUDGET: Final = 16_000
MAP_PROGRESS_START: Final = 35.0
MAP_PROGRESS_CEILING: Final = 68.0
REDUCE_PROGRESS_START: Final = 70.0
REDUCE_PROGRESS_CEILING: Final = 88.0
# Each chunk starts mid-conversation, so it carries the tail of the previous
# one as read-only context. Without it a decision stated just before the cut
# reads as an unattributed follow-up in the next chunk's notes.
CHUNK_OVERLAP_CHARS: Final = 400
# The provider is the bottleneck, not the CPU; a small pool keeps a long
# meeting's map pass from running strictly one request at a time.
MAP_MAX_WORKERS: Final = 3

RefinementStrategy = Literal["single", "map_reduce"]


class ChatMessage(TypedDict):
    """One OpenAI-compatible chat message."""

    role: str
    content: str


@dataclass(frozen=True, slots=True)
class MinutesContext:
    """Prompt-ready meeting context shared by map and reduce passes."""

    background: str
    participants: str
    glossary: str
    meeting_date: str | None


@dataclass(frozen=True, slots=True)
class TranscriptChunk:
    """One whole-turn chunk and its position in a long transcript."""

    number: int
    total: int
    text: str
    preamble: str = ""


MAP_SYSTEM_PROMPT: Final = """당신은 긴 한국어 회의 전사본의 한 구간에서 최종 회의록에 필요한 사실만 추출합니다.

규칙:
- 최종 회의록을 쓰지 말고 이 구간에서 확인되는 사실 후보만 구조화합니다.
- 결정, 할 일, 주제별 논의, 열린 질문과 리스크, 용어 보정, 화자 보정 후보를 구분합니다.
- 각 후보에는 가능한 경우 화자 라벨과 시각, 짧은 근거 인용을 남깁니다.
- 구간에 없는 내용, 담당, 날짜, 실명을 추정하지 않습니다. `목요일까지` 같은 상대적인 기한은 원래 표현대로 남깁니다.
- 참석자 명단 밖의 실명을 만들지 않습니다.
- 앞뒤 구간이 있어야 확정할 수 있는 내용은 `확인 필요`로 표시합니다.
- 민감정보는 `[민감정보 생략]`으로 가립니다.
- 정보가 없는 항목은 `해당 없음`으로 둡니다.
"""


def refinement_strategy(transcript: str) -> RefinementStrategy:
    """Route only transcripts above the proven single-pass limit to map/reduce."""

    return "map_reduce" if len(transcript) > SINGLE_PASS_CHAR_LIMIT else "single"


def map_progress_band(chunk: TranscriptChunk) -> tuple[float, float]:
    """Return one rounded map subband with boundaries shared by adjacent chunks."""

    span = MAP_PROGRESS_CEILING - MAP_PROGRESS_START
    start = round(MAP_PROGRESS_START + span * ((chunk.number - 1) / chunk.total), 1)
    ceiling = round(MAP_PROGRESS_START + span * (chunk.number / chunk.total), 1)
    return start, ceiling


def split_transcript(
    transcript: str, *, max_chars: int = CHUNK_CHAR_BUDGET
) -> list[TranscriptChunk]:
    """Pack whole speaker-turn lines into bounded chunks without mid-turn cuts."""

    turns = [line.strip() for line in transcript.strip().splitlines() if line.strip()]
    packed: list[str] = []
    current: list[str] = []
    current_chars = 0
    for turn in turns:
        separator_chars = 1 if current else 0
        if current and current_chars + separator_chars + len(turn) > max_chars:
            packed.append("\n".join(current))
            current = [turn]
            current_chars = len(turn)
        else:
            current.append(turn)
            current_chars += separator_chars + len(turn)
    if current:
        packed.append("\n".join(current))
    total = len(packed)
    return [
        TranscriptChunk(
            number=index + 1,
            total=total,
            text=text,
            preamble=packed[index - 1][-CHUNK_OVERLAP_CHARS:] if index else "",
        )
        for index, text in enumerate(packed)
    ]


def build_map_messages(
    chunk: TranscriptChunk, context: MinutesContext
) -> list[ChatMessage]:
    """Build one fact-extraction request for a long-transcript chunk."""

    date = context.meeting_date or "미정"
    preamble = (
        f"<이전구간끝>\n{chunk.preamble}\n</이전구간끝>\n"
        "위 이전구간끝은 맥락 참고용입니다. 여기서 사실을 추출하지 마세요.\n\n"
        if chunk.preamble
        else ""
    )
    user = (
        f'<회의정보 날짜="{date}">\n'
        f"<사전정보>\n{context.background}\n</사전정보>\n"
        f"<참석자>\n{context.participants}\n</참석자>\n"
        f"<단어집>\n{context.glossary}\n</단어집>\n"
        f"</회의정보>\n\n"
        f"{preamble}"
        f'<전사구간 번호="{chunk.number}" 전체="{chunk.total}">\n'
        f"{chunk.text}\n"
        "</전사구간>\n\n"
        "이 구간의 사실 후보를 결정/할 일/주제/열린 질문과 리스크/보정 후보로 구조화하세요."
    )
    return [
        {"role": "system", "content": MAP_SYSTEM_PROMPT},
        {"role": "user", "content": user},
    ]


def build_reduce_messages(
    partial_notes: Sequence[str], context: MinutesContext, system_prompt: str
) -> list[ChatMessage]:
    """Build the final composition request from ordered map-pass notes."""

    date = context.meeting_date or "미정"
    notes = "\n\n".join(
        f'<부분노트 번호="{index}">\n{note}\n</부분노트>'
        for index, note in enumerate(partial_notes, start=1)
    )
    user = (
        f"회의 추정일: {date}\n"
        f"<사전정보>\n{context.background}\n</사전정보>\n\n"
        f"<참석자>\n{context.participants}\n</참석자>\n\n"
        f"<단어집>\n{context.glossary}\n</단어집>\n\n"
        "아래 부분 노트들은 긴 전사본의 시간순 구간에서 추출한 사실 후보입니다. "
        "중복을 합치고, 뒤 구간에서 수정·철회된 앞 구간 결정을 최신 상태로 정리하세요. "
        "부분 노트에 없는 사실을 만들지 마세요.\n\n"
        f"{notes}\n\n"
        "위 규칙과 형식에 맞춰 최종 회의록 Markdown 문서 하나를 작성하세요."
    )
    return [
        {"role": "system", "content": system_prompt},
        {"role": "user", "content": user},
    ]
