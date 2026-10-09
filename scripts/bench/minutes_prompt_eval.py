"""Deterministic rubric for the minutes refinement prompts.

Scores the prompt text itself (no model call, no network) on four axes and
subtracts a size penalty for request overhead:

- structure (25): the format spec, its exemplar, the user-message section
  list, and the map-pass categories agree, and the format carries every
  information type a team-shareable minutes document needs;
- exemplar Korean (25): the exemplar the model imitates follows
  fluent-korean (complete sentences where prose is expected, no em dash,
  no English labels in a Korean document, no slop markers);
- instruction Korean (20): the instruction prose itself follows fluent-korean;
- style rules transmitted (15): the system prompt tells the model the
  fluent-korean output rules;
- fidelity rules kept (15): regression guard for the existing safety rules.

Prints `METRIC name=value` lines. Higher `prompt_score` is better.
"""

import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "worker"))

from galpi_worker.minutes_pipeline import (
    MAP_SYSTEM_PROMPT,
    MinutesContext,
    TranscriptChunk,
    build_map_messages,
    build_reduce_messages,
)
from galpi_worker.minutes_prompt import build_messages
from galpi_worker.minutes_template import SYSTEM_PROMPT

SIZE_BUDGET_CHARS = 4400
SIZE_PENALTY_CHARS_PER_POINT = 40

H1 = re.compile(r"^# ", re.MULTILINE)
H2 = re.compile(r"^## (.+?)\s*$", re.MULTILINE)
BACKTICK = re.compile(r"`[^`]*`")
QUOTED = re.compile(r'"[^"]*"')
PLACEHOLDER = re.compile(r"\{[^}]*\}")

# Information types a shared minutes document must carry, matched on H2 names.
REQUIRED_INFO = {
    "summary": r"TL;DR|요약",
    "decisions": r"결정",
    "actions": r"액션|할 일|실행",
    "discussion": r"논의",
    "open_questions": r"리스크|열린 질문|미결|확인 필요",
    "corrections": r"보정",
}
# Map-pass categories that must exist so reduce can fill each body section.
MAP_CATEGORIES = {
    "decisions": r"결정",
    "actions": r"액션|할 일",
    "discussion": r"주제|논의",
    "open_questions": r"리스크|열린 질문",
    "term_fix": r"용어 보정",
    "speaker_fix": r"화자 보정",
}
# Fluent-korean output rules the system prompt must hand to the model.
STYLE_RULES = {
    "complete_sentences": r"종결어미|완결된 문장",
    "no_hedging": r"것 같|단정을 회피|추측 어미",
    "no_empty_modifiers": r"수식어|강조어",
    "no_em_dash": r"엠대시",
    "no_translationese": r"번역투|되어지|이중피동",
    "consistent_terms": r"같은 (개념|대상)|용어를 일관|하나의 (용어|표기)",
}
# Existing safety/fidelity rules that must survive any rewrite.
FIDELITY_RULES = {
    "no_invented_names": r"실명",
    "empty_section_marker": r"`해당 없음`",
    "unknown_marker": r"`미정`",
    "redaction": r"\[민감정보 생략\]",
    "estimate_marker": r"추정",
    "no_code_fence": r"코드펜스",
    "evidence": r"근거",
    "glossary": r"단어집",
    "roster": r"참석자 명단",
}
ENGLISH_LABELS = re.compile(r"\b(Source|Status|Tracking|Draft|follow-up|Owner|Due)\b")
METAPHORS = re.compile(r"훈증|우겨넣|박는|녹여|쏟아|녹아")
HEDGES = re.compile(r"것 같|수 있을 것|라고 할 수|듯합니다|듯하다")
TRANSLATIONESE = re.compile(r"되어지|에 의해|에 있어서|을 통해서|를 통해서")
CONNECTIVES = re.compile(r"따라서|또한|이를 통해")
EMPTY_MODIFIERS = re.compile(r"다양한|전반적|효과적|적극적|매우 |굉장히")
# `회의`, `논의` etc. end in 의 but are nouns, not genitive markers.
GENITIVE_CHAIN = re.compile(
    r"[가-힣]*[^회주논동합정협질건의\s]의 [가-힣]*[^회주논동합정협질건의\s]의 "
)
NOMINAL_END = re.compile(
    r"(음|함|됨|임|짐|확정|조건|필요|미비|점검|정리|추가|이관|확인|부재)\s*\.?$"
)
SENTENCE_END = re.compile(r"다\.$")
# Field labels whose value is prose and must be a complete sentence.
SENTENCE_FIELDS = re.compile(r"^\s*- (내용|논점|정리|배경|영향): (.+)$")


@dataclass
class Report:
    points: float = 0.0
    maximum: float = 0.0
    findings: list[str] = field(default_factory=list)

    def award(self, weight: float, ok: bool, label: str) -> None:
        self.maximum += weight
        if ok:
            self.points += weight
        else:
            self.findings.append(label)


def strip_inline(text: str) -> str:
    """Remove quoted examples so banned words cited as examples are not flagged."""

    return QUOTED.sub("", BACKTICK.sub("", text))


def split_system_prompt(prompt: str) -> tuple[str, str, str]:
    """Return (instruction prose, format template, exemplar document)."""

    starts = [match.start() for match in H1.finditer(prompt)]
    if len(starts) < 2:
        return prompt, "", ""
    prose = prompt[: starts[0]]
    template = prompt[starts[0] : starts[1]]
    exemplar = prompt[starts[1] :]
    # Lines of the template block that are not Markdown structure are prose
    # (e.g. the sentence introducing the exemplar).
    structural = ("#", "-", ">", "(", "{", "  ", "---", "|")
    tail = [
        line
        for line in template.splitlines()
        if line.strip() and not line.startswith(structural)
    ]
    return prose + "\n" + "\n".join(tail), template, exemplar


def sentences(text: str) -> list[str]:
    """Split prose into rule sentences (bullets and period-terminated clauses)."""

    out: list[str] = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.endswith(":"):
            continue
        line = line.removeprefix("- ")
        for part in re.split(r"(?<=\.)\s+", line):
            if part.strip():
                out.append(part.strip())
    return out


def lint_prose(label: str, text: str, report: Report, weight: float) -> None:
    """Penalize fluent-korean violations in instruction prose."""

    bare = strip_inline(PLACEHOLDER.sub("", text))
    violations: list[str] = []
    violations += [f"{label}: em dash"] * bare.count("—")
    violations += [f"{label}: metaphor {m}" for m in METAPHORS.findall(bare)]
    violations += [f"{label}: hedge {m}" for m in HEDGES.findall(bare)]
    violations += [f"{label}: translationese {m}" for m in TRANSLATIONESE.findall(bare)]
    violations += [
        f"{label}: empty modifier {m}" for m in EMPTY_MODIFIERS.findall(bare)
    ]
    violations += [f"{label}: genitive chain {m}" for m in GENITIVE_CHAIN.findall(bare)]
    extra_connectives = max(0, len(CONNECTIVES.findall(bare)) - 1)
    violations += [f"{label}: connective overuse"] * extra_connectives
    for sentence in sentences(strip_inline(text)):
        if not sentence.rstrip(".").endswith(("다", "요")):
            violations.append(f"{label}: incomplete sentence «{sentence[-24:]}»")
    score = max(0.0, weight - 2.0 * len(violations))
    report.points += score
    report.maximum += weight
    report.findings += violations


def lint_document(label: str, text: str, report: Report, weight: float) -> None:
    """Penalize fluent-korean violations in a Markdown minutes document."""

    violations: list[str] = []
    violations += [f"{label}: em dash"] * text.count("—")
    violations += [f"{label}: english label {m}" for m in ENGLISH_LABELS.findall(text)]
    bare = strip_inline(PLACEHOLDER.sub("", text))
    violations += [f"{label}: metaphor {m}" for m in METAPHORS.findall(bare)]
    violations += [f"{label}: hedge {m}" for m in HEDGES.findall(bare)]
    violations += [f"{label}: translationese {m}" for m in TRANSLATIONESE.findall(bare)]
    violations += [
        f"{label}: empty modifier {m}" for m in EMPTY_MODIFIERS.findall(bare)
    ]
    section = ""
    for line in text.splitlines():
        heading = H2.match(line)
        if heading:
            section = heading.group(1)
            continue
        if "{" in line:
            continue
        value = None
        fieldmatch = SENTENCE_FIELDS.match(line)
        if fieldmatch:
            value = fieldmatch.group(2).strip()
        elif re.search(REQUIRED_INFO["summary"] + "|목적", section) and line.startswith(
            "- "
        ):
            value = line[2:].strip()
        if value is None or value == "`해당 없음`" or value == "해당 없음":
            continue
        if not SENTENCE_END.search(value) or NOMINAL_END.search(value):
            violations.append(f"{label}: not a sentence «{value[-24:]}»")
        head = value.split(",")[0]
        if re.search(r"[가-힣],\s", value) and NOMINAL_END.search(head):
            violations.append(f"{label}: comma splice «{value[:24]}»")
    score = max(0.0, weight - 2.0 * len(violations))
    report.points += score
    report.maximum += weight
    report.findings += violations


def ordered_subsequence(needles: list[str], haystack: str) -> bool:
    cursor = 0
    for needle in needles:
        found = haystack.find(needle, cursor)
        if found < 0:
            return False
        cursor = found + len(needle)
    return True


def main() -> int:
    transcript = "[SPEAKER_00] (0s) 안녕하세요.\n[SPEAKER_01] (3s) 시작하겠습니다."
    single = build_messages(transcript, "", [], [], "2026-01-15")
    context = MinutesContext(
        background="(없음)",
        participants="(없음)",
        glossary="(없음)",
        meeting_date="2026-01-15",
    )
    chunk = TranscriptChunk(number=1, total=2, text=transcript)
    map_user = build_map_messages(chunk, context)[1]["content"]
    reduce_user = build_reduce_messages(["노트"], context, SYSTEM_PROMPT)[1]["content"]
    single_user = single[1]["content"]
    scaffold_single = single_user.replace(transcript, "")
    scaffold_map = map_user.replace(transcript, "")

    prose, template, exemplar = split_system_prompt(SYSTEM_PROMPT)
    template_sections = H2.findall(template)
    exemplar_sections = H2.findall(exemplar)

    structure = Report()
    structure.award(
        2, bool(template) and bool(exemplar), "split: template/exemplar not found"
    )
    structure.award(
        6,
        template_sections == exemplar_sections and len(template_sections) > 0,
        f"exemplar sections {exemplar_sections} != template {template_sections}",
    )
    for key, pattern in REQUIRED_INFO.items():
        structure.award(
            1,
            any(re.search(pattern, s) for s in template_sections),
            f"format lacks {key}",
        )
    body_sections = [s for s in template_sections if not re.search("보정", s)]
    mentioned = [s for s in template_sections if s in scaffold_single]
    structure.award(
        3,
        not mentioned
        or (
            mentioned == template_sections
            and ordered_subsequence(template_sections, scaffold_single)
        ),
        "single-pass user message section list drifts from the template",
    )
    structure.award(
        2,
        "보정" not in "".join(body_sections) and len(body_sections) >= 5,
        "body sections missing",
    )
    for key, pattern in MAP_CATEGORIES.items():
        structure.award(
            0.5, bool(re.search(pattern, MAP_SYSTEM_PROMPT)), f"map prompt lacks {key}"
        )
    structure.award(
        1,
        bool(re.search(r"\{[^}]*(일시|날짜)[^}]*\}", template))
        and bool(re.search(r"참석", template)),
        "header lacks date/attendees",
    )
    structure.award(
        2,
        bool(re.search(r"^- \[ \] .*기한", template, re.MULTILINE)),
        "action item lacks checkbox+due",
    )

    exemplar_quality = Report()
    lint_document("exemplar", exemplar, exemplar_quality, 20)
    lint_document("template", template, exemplar_quality, 5)

    prose_quality = Report()
    lint_prose("system", prose, prose_quality, 12)
    lint_prose("map", MAP_SYSTEM_PROMPT, prose_quality, 4)
    user_tails = "\n".join(
        line
        for text in (scaffold_single, scaffold_map, reduce_user)
        for line in text.splitlines()
        if line
        and not line.startswith(("<", "-", "[", "("))
        and "회의 추정일" not in line
        and "노트" != line
    )
    lint_prose("user", user_tails, prose_quality, 4)

    style = Report()
    for key, pattern in STYLE_RULES.items():
        style.award(2.5, bool(re.search(pattern, prose)), f"style rule missing: {key}")

    fidelity = Report()
    for key, pattern in FIDELITY_RULES.items():
        fidelity.award(
            15 / len(FIDELITY_RULES),
            bool(re.search(pattern, SYSTEM_PROMPT)),
            f"fidelity rule missing: {key}",
        )

    prompt_chars = len(SYSTEM_PROMPT) + len(MAP_SYSTEM_PROMPT) + len(scaffold_single)
    penalty = max(0, prompt_chars - SIZE_BUDGET_CHARS) / SIZE_PENALTY_CHARS_PER_POINT
    parts = {
        "structure": structure,
        "exemplar_korean": exemplar_quality,
        "instruction_korean": prose_quality,
        "style_rules": style,
        "fidelity_rules": fidelity,
    }
    total = sum(part.points for part in parts.values()) - penalty
    for name, part in parts.items():
        for finding in part.findings:
            print(f"FINDING [{name}] {finding}", file=sys.stderr)
    print(f"METRIC prompt_score={total:.2f}")
    for name, part in parts.items():
        print(f"METRIC {name}={part.points:.2f}")
    print(f"METRIC prompt_chars={prompt_chars}")
    print(f"METRIC system_prompt_chars={len(SYSTEM_PROMPT)}")
    print(f"METRIC size_penalty={penalty:.2f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
