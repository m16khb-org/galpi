"""Typed meeting context parsing and single-pass prompt construction."""

import time
from dataclasses import dataclass
from pathlib import Path
from typing import cast

from .core import InvalidInput
from .minutes_pipeline import ChatMessage
from .minutes_template import SYSTEM_PROMPT

NO_BACKGROUND = "(등록된 사전 정보가 없습니다. 전사본에 있는 근거만 사용하세요.)"
NO_PARTICIPANTS = "(선택된 참석자가 없습니다. 전사본에 있는 근거만 사용하세요.)"
NO_GLOSSARY = "(등록된 용어가 없습니다. 전사본에 있는 근거만 사용하세요.)"


@dataclass(frozen=True)
class Participant:
    """One meeting attendee selected from the saved roster."""

    name: str
    team: str | None
    role: str | None
    description: str | None
    aliases: tuple[str, ...]


@dataclass(frozen=True)
class GlossaryEntry:
    """One saved glossary term applied to every refinement."""

    term: str
    description: str | None


def optional_text(value: object) -> str | None:
    """Trim a roster field to text, treating blank as absent."""

    if isinstance(value, str) and value.strip():
        return value.strip()
    return None


def parse_participants(payload: object) -> list[Participant]:
    """Convert the roster JSON handed over by the host into typed participants."""

    if not isinstance(payload, list):
        raise TypeError("participants payload was not a JSON array")
    participants: list[Participant] = []
    for entry in cast(list[object], payload):
        if not isinstance(entry, dict):
            raise TypeError("participant entry was not a JSON object")
        fields = cast(dict[str, object], entry)
        name = fields.get("name")
        if not isinstance(name, str) or not name.strip():
            raise InvalidInput("participant entry had no name")
        team = optional_text(fields.get("team"))
        role = optional_text(fields.get("role"))
        description = optional_text(fields.get("description"))
        raw_aliases = fields.get("aliases")
        aliases = (
            tuple(
                alias.strip()
                for alias in cast(list[object], raw_aliases)
                if isinstance(alias, str) and alias.strip()
            )
            if isinstance(raw_aliases, list)
            else ()
        )
        participants.append(
            Participant(
                name=name.strip(),
                team=team,
                role=role,
                description=description,
                aliases=aliases,
            )
        )
    return participants


def parse_glossary(payload: object) -> list[GlossaryEntry]:
    """Convert the glossary JSON handed over by the host into typed entries."""

    if not isinstance(payload, list):
        raise TypeError("glossary payload was not a JSON array")
    entries: list[GlossaryEntry] = []
    for entry in cast(list[object], payload):
        if not isinstance(entry, dict):
            raise TypeError("glossary entry was not a JSON object")
        fields = cast(dict[str, object], entry)
        term = fields.get("term")
        if not isinstance(term, str) or not term.strip():
            raise InvalidInput("glossary entry had no term")
        entries.append(
            GlossaryEntry(
                term=term.strip(), description=optional_text(fields.get("description"))
            )
        )
    return entries


def render_participants(participants: list[Participant]) -> str:
    """Render the attendee roster as one prompt block."""

    if not participants:
        return NO_PARTICIPANTS
    lines: list[str] = []
    for participant in participants:
        detail = " · ".join(
            part for part in (participant.team, participant.role) if part is not None
        )
        line = f"- {participant.name}" + (f" ({detail})" if detail else "")
        if participant.aliases:
            line += f" / 별칭: {', '.join(participant.aliases)}"
        if participant.description is not None:
            line += f" / 설명: {participant.description}"
        lines.append(line)
    return "\n".join(lines)


def render_glossary(entries: list[GlossaryEntry]) -> str:
    """Render the glossary as one prompt block."""

    if not entries:
        return NO_GLOSSARY
    return "\n".join(
        f"- {entry.term}: {entry.description}"
        if entry.description is not None
        else f"- {entry.term}"
        for entry in entries
    )


def transcript_date(path: Path) -> str | None:
    """Best-effort meeting date from the transcript file mtime (local date)."""

    try:
        return time.strftime("%Y-%m-%d", time.localtime(path.stat().st_mtime))
    except OSError:
        return None


def build_messages(
    transcript: str,
    background: str,
    participants: list[Participant],
    glossary: list[GlossaryEntry],
    meeting_date: str | None = None,
) -> list[ChatMessage]:
    """Build the chat messages for one refinement request."""

    if not transcript.strip():
        raise InvalidInput("transcript is empty")
    context = background.strip() or NO_BACKGROUND
    date_line = (
        f"회의 추정일: {meeting_date} (녹음 파일 기준). 전사본에 이와 다른 명시적 날짜 근거가 없으면 이 값을 그대로 사용하세요.\n\n"
        if meeting_date is not None
        else ""
    )
    user = (
        date_line + "다음은 회의 참석자, 제품/서비스, 용어에 대한 사전 정보입니다.\n"
        "<사전정보>\n"
        f"{context}\n"
        "</사전정보>\n\n"
        "다음은 이 회의에 참석한 사람들입니다. 화자 실명 후보는 이 명단 안에서만 찾으세요.\n"
        "<참석자>\n"
        f"{render_participants(participants)}\n"
        "</참석자>\n\n"
        "다음은 이 팀이 자주 쓰는 용어의 단어집입니다. 등록된 표기를 그대로 따르세요.\n"
        "<단어집>\n"
        f"{render_glossary(glossary)}\n"
        "</단어집>\n\n"
        "다음은 화자분리된 회의 전사본입니다.\n"
        "<전사본>\n"
        f"{transcript.strip()}\n"
        "</전사본>\n\n"
        "위 규칙과 문서 구조에 맞춰 회의록을 Markdown 문서로 작성하세요."
    )
    return [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": user},
    ]
