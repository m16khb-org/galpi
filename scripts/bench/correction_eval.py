"""Deterministic benchmark for transcript-correction support in AI augmentation.

Galpi's transcripts arrive with ASR misrecognitions (glossary terms, product
names from the background, attendee names, jargon no glossary lists),
anonymous speaker labels, and diarization errors. The LLM fixes them only as
well as the request lets it: this harness measures what `plan_refinement`
actually sends for a seeded corpus of noisy meetings with known gold (see
correction_corpus.py), without calling a model. Every run averages several
noise seeds, and half of the misspellings are generated procedurally from
jamo confusions, so a detector has to generalize rather than look words up.

Correction evidence is read from `<보정후보>` blocks in the user messages, one
candidate per line:

    - 용어: "{ASR 표기}" → "{올바른 표기}" (근거 ...)
    - 이름: "{ASR 표기}" → "{명단 이름}" (근거 ...)
    - 같은 표현: "{표기 A}" = "{표기 B}" = ... (근거 ...)
    - 화자: SPEAKER_01 → {명단 이름 또는 확인 필요} (근거 ...)
    - 화자 경계: ({초}s) SPEAKER_02 → SPEAKER_00 (근거 ...)

Both the single-pass request and the map/reduce requests are scored (the map
path is forced), so evidence must reach every path the worker can take.

Components (weights sum to 100, higher is better):
- term_f1 (25): glossary/background misrecognitions paired with the right term;
- name_f1 (15): misrecognized attendee names paired with the roster name,
  keeping an attached honorific (`승인님` is a name error, `승인` a real word);
- cluster_f1 (10): spellings of the same unlisted jargon grouped together;
- speaker_score (20): speaker labels mapped to the right roster name. Wrong
  costs a point, abstaining costs nothing; a guest label must stay unmapped;
- boundary_f1 (15): lines whose label is wrong (split tails, replies stamped
  with the asker's label or UNKNOWN) attributed to the right speaker;
- compaction (5): fewer transcript lines (merged same-speaker segments) with
  the transcript text preserved (similarity >= 0.98), saturating at 40% fewer;
- instructions (10): both system prompts tell the model how to use the
  candidates (verify against context, record what was applied).

Overfitting guard: if worker source gains any corpus-specific word (hand
variant spellings, distractors, roster names, unlisted jargon) beyond the
counts in correction_guard_baseline.json, the score is 0.

Prints `METRIC name=value` lines.
"""

import json
import re
import sys
from collections import Counter
from collections.abc import Iterable
from dataclasses import dataclass
from difflib import SequenceMatcher
from itertools import combinations
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / "worker"))
sys.path.insert(0, str(HERE))

from correction_corpus import MEETINGS, SEEDS, Generated, corpus
from galpi_worker.minutes_pipeline import ChatMessage
from galpi_worker.minutes_prompt import (
    parse_glossary,
    parse_participants,
    plan_refinement,
)

WEIGHTS = {
    "term_f1": 25.0,
    "name_f1": 15.0,
    "cluster_f1": 10.0,
    "speaker_score": 20.0,
    "boundary_f1": 15.0,
    "compaction": 5.0,
    "instructions": 10.0,
}
COMPACTION_SATURATION = 0.4
PRESERVED_SIMILARITY = 0.98
MEETING_DATE = "2026-10-08"
WORKER_SOURCE = ROOT / "worker" / "galpi_worker"
GUARD_BASELINE = HERE / "correction_guard_baseline.json"

BLOCK = re.compile(r"<보정후보>(.*?)</보정후보>", re.DOTALL)
LEXICAL = re.compile(
    r'^\s*-\s*(?:용어|이름)\s*:\s*"([^"]+)"\s*→\s*"([^"]+)"', re.MULTILINE
)
CLUSTER = re.compile(r"^\s*-\s*같은 표현\s*:(.*)$", re.MULTILINE)
QUOTED = re.compile(r'"([^"]+)"')
SPEAKER = re.compile(
    r"^\s*-\s*화자\s*:\s*(SPEAKER_\d+)\s*→\s*([^\s(,.]+)", re.MULTILINE
)
BOUNDARY = re.compile(
    r"^\s*-\s*화자 경계\s*:\s*\((\d+)s\)\s*(?:SPEAKER_\d+|UNKNOWN)\s*→\s*(SPEAKER_\d+)",
    re.MULTILINE,
)
TRANSCRIPT_BLOCK = re.compile(
    r"<(?:전사본|전사구간[^>]*)>\n(.*?)\n</(?:전사본|전사구간)>", re.DOTALL
)
LINE_PREFIX = re.compile(r"\[(?:SPEAKER_\d+|UNKNOWN)\]|\(\d+s\)|\s+")
# Grammatical particles only: an honorific stays part of a name surface.
PARTICLES = (
    "께서",
    "에게",
    "이랑",
    "랑",
    "으로",
    "로",
    "이",
    "가",
    "은",
    "는",
    "을",
    "를",
    "의",
    "에",
    "도",
    "와",
    "과",
)
HONORIFICS = ("님", "씨")
ABSTAIN = {"확인", "확인필요", "미상", "알수없음"}


def strip_particle(text: str) -> str:
    compact = text.strip()
    for particle in PARTICLES:
        if compact.endswith(particle) and len(compact) > len(particle) + 1:
            return compact[: -len(particle)].strip()
    return compact


def without_honorific(text: str) -> str:
    compact = text.strip()
    for honorific in HONORIFICS:
        if compact.endswith(honorific) and len(compact) > len(honorific):
            return compact[: -len(honorific)].strip()
    return compact


def canonical(text: str, known: set[str]) -> str:
    """The known form of a hinted word: as written, or with a particle removed.

    Gold keeps words exactly as transcribed (`처우 협의` must not lose its
    last syllable), so only the hint side falls back to particle stripping.
    """

    compact = text.strip()
    if compact in known:
        return compact
    stripped = strip_particle(compact)
    return stripped if stripped in known else compact


def hint_text(requests: list[list[ChatMessage]]) -> str:
    return "\n".join(
        block
        for messages in requests
        for message in messages
        if message["role"] == "user"
        for block in BLOCK.findall(message["content"])
    )


@dataclass
class Counts:
    tp: int = 0
    fp: int = 0
    fn: int = 0

    def add(self, other: "Counts") -> None:
        self.tp += other.tp
        self.fp += other.fp
        self.fn += other.fn

    def f1(self) -> float:
        if self.tp == 0:
            return 0.0
        precision = self.tp / (self.tp + self.fp)
        recall = self.tp / (self.tp + self.fn)
        return 2 * precision * recall / (precision + recall)


def compare(gold: set[tuple[str, str]], mine: set[tuple[str, str]]) -> Counts:
    return Counts(tp=len(mine & gold), fp=len(mine - gold), fn=len(gold - mine))


@dataclass
class PathScore:
    terms: Counts
    names: Counts
    clusters: Counts
    boundaries: Counts
    speaker_points: float
    speaker_labels: int
    distractor_hits: int
    lines: int
    preserved: bool


def roster_identities(generated: Generated) -> dict[str, str]:
    """Every way the roster names a person, mapped to the roster name."""

    names: dict[str, str] = {}
    for person in generated.meeting.people:
        if not person.on_roster:
            continue
        forms = [person.name, *person.aliases]
        forms += [alias.split()[0] for alias in person.aliases]
        # A three-syllable Korean name is often used without the surname.
        if len(person.name) == 3 and all("가" <= c <= "힣" for c in person.name):
            forms.append(person.name[1:])
        for form in forms:
            names[without_honorific(form).replace(" ", "")] = person.name
    return names


def score_path(generated: Generated, requests: list[list[ChatMessage]]) -> PathScore:
    hints = hint_text(requests)
    roster = roster_identities(generated)

    gold_terms = {
        (error.surface, without_honorific(error.correct))
        for error in generated.lexical_errors
        if error.kind in ("term", "background")
    }
    gold_names = {
        (error.surface, without_honorific(error.correct))
        for error in generated.lexical_errors
        if error.kind == "name"
    }
    gold_cluster_pairs = {
        frozenset(pair)
        for surfaces in generated.clusters.values()
        for pair in combinations(sorted(surfaces), 2)
    }
    surfaces = {surface for surface, _ in gold_terms | gold_names} | {
        word for pair in gold_cluster_pairs for word in pair
    }
    targets = {target for _, target in gold_terms | gold_names} | set(roster)

    proposed = {
        (canonical(s, surfaces), without_honorific(canonical(t, targets)))
        for s, t in LEXICAL.findall(hints)
    }
    proposed = {(s, t) for s, t in proposed if s != t}
    cluster_pairs: set[frozenset[str]] = set()
    for line in CLUSTER.findall(hints):
        members = sorted({canonical(word, surfaces) for word in QUOTED.findall(line)})
        cluster_pairs.update(frozenset(pair) for pair in combinations(members, 2))
    # A lexical hint between two spellings of the same unlisted word is a
    # grouping, not a term correction.
    for surface, target in proposed:
        if frozenset((surface, target)) in gold_cluster_pairs:
            cluster_pairs.add(frozenset((surface, target)))
    lexical = {
        (s, t) for s, t in proposed if frozenset((s, t)) not in gold_cluster_pairs
    }
    name_hints = {(s, t) for s, t in lexical if t.replace(" ", "") in roster}
    term_hints = lexical - name_hints

    clusters = Counts(
        tp=len(cluster_pairs & gold_cluster_pairs),
        fp=len(cluster_pairs - gold_cluster_pairs),
        fn=len(gold_cluster_pairs - cluster_pairs),
    )

    speaker_hints: dict[str, set[str]] = {}
    for label, name in SPEAKER.findall(hints):
        speaker_hints.setdefault(label, set()).add(name.strip())
    points = 0.0
    for label, gold in generated.speaker_names.items():
        keys = {
            name: without_honorific(canonical(name, set(roster))).replace(" ", "")
            for name in speaker_hints.get(label, set())
        }
        guesses = {
            roster.get(key, name) for name, key in keys.items() if key not in ABSTAIN
        }
        if not guesses:
            points += 1.0 if gold is None else 0.0
        elif gold is not None and guesses == {gold}:
            points += 1.0
        else:
            points -= 1.0

    gold_boundaries = {
        (error.second, error.true_label) for error in generated.boundary_errors
    }
    proposed_boundaries = {
        (int(second), label) for second, label in BOUNDARY.findall(hints)
    }

    sent = "\n".join(
        block
        for messages in requests
        for message in messages
        if message["role"] == "user"
        for block in TRANSCRIPT_BLOCK.findall(message["content"])
    )
    original = LINE_PREFIX.sub("", generated.transcript)
    rendered = LINE_PREFIX.sub("", sent)
    preserved = (
        SequenceMatcher(None, original, rendered, autojunk=False).ratio()
        >= PRESERVED_SIMILARITY
    )
    distractors = {strip_particle(word) for word in generated.meeting.distractors}

    return PathScore(
        terms=compare(gold_terms, term_hints),
        names=compare(gold_names, name_hints),
        clusters=clusters,
        boundaries=compare(gold_boundaries, proposed_boundaries),
        speaker_points=points,
        speaker_labels=len(generated.speaker_names),
        distractor_hits=sum(1 for surface, _ in proposed if surface in distractors),
        lines=len([line for line in sent.splitlines() if line.strip()]),
        preserved=preserved,
    )


INSTRUCTION_CHECKS = (
    ("single_mentions_candidates", lambda single, mapped: "보정후보" in single),
    ("map_mentions_candidates", lambda single, mapped: "보정후보" in mapped),
    (
        "verify_against_context",
        lambda single, mapped: all(
            re.search(r"보정후보[^\n]*(문맥|맥락)", prompt)
            for prompt in (single, mapped)
        ),
    ),
    (
        "record_applied",
        lambda single, mapped: (
            re.search(r"보정후보[^\n]*보정 기록", single) is not None
            and re.search(r"보정후보[^\n]*보정", mapped) is not None
        ),
    ),
)


def system_prompts(requests: Iterable[list[ChatMessage]]) -> str:
    return "\n".join(
        message["content"]
        for messages in requests
        for message in messages
        if message["role"] == "system"
    )


def guard_words() -> set[str]:
    """Corpus-specific words: hand variants, distractors, names, unlisted jargon."""

    words: set[str] = set()
    for meeting in MEETINGS:
        for table in (
            meeting.term_variants,
            meeting.background_variants,
            meeting.name_variants,
        ):
            for options in table.values():
                words.update(options)
        for surface, options in meeting.unlisted_variants.items():
            words.add(surface)
            words.update(options)
        words.update(meeting.distractors)
        for person in meeting.people:
            if person.on_roster:
                words.add(person.name)
                words.update(person.aliases)
    return {word for word in words if len(word) >= 2}


def guard_counts() -> Counter[str]:
    source = "\n".join(
        path.read_text(encoding="utf-8") for path in sorted(WORKER_SOURCE.glob("*.py"))
    )
    return Counter({word: source.count(word) for word in guard_words()})


def main() -> int:
    if "--write-guard-baseline" in sys.argv:
        GUARD_BASELINE.write_text(
            json.dumps(
                dict(sorted(guard_counts().items())), ensure_ascii=False, indent=1
            )
            + "\n",
            encoding="utf-8",
        )
        return 0
    baseline: dict[str, int] = json.loads(GUARD_BASELINE.read_text(encoding="utf-8"))
    leaked = sorted(
        word for word, count in guard_counts().items() if count > baseline.get(word, 0)
    )

    terms, names, clusters, boundaries = Counts(), Counts(), Counts(), Counts()
    speaker_points = 0.0
    speaker_labels = 0
    distractor_hits = 0
    compactions: list[float] = []
    request_chars = 0
    single_system = map_system = ""
    for seed in SEEDS:
        for generated in corpus(seed):
            participants = parse_participants(generated.participants_payload())
            glossary = parse_glossary(generated.glossary_payload())
            for strategy in ("single", "map_reduce"):
                plan = plan_refinement(
                    generated.transcript,
                    generated.meeting.background,
                    participants,
                    glossary,
                    MEETING_DATE,
                    strategy=strategy,
                )
                requests = plan.requests()
                if strategy == "single":
                    single_system = system_prompts(requests)
                    request_chars += sum(len(m["content"]) for r in requests for m in r)
                else:
                    map_system = system_prompts(requests)
                score = score_path(generated, requests)
                terms.add(score.terms)
                names.add(score.names)
                clusters.add(score.clusters)
                boundaries.add(score.boundaries)
                speaker_points += score.speaker_points
                speaker_labels += score.speaker_labels
                distractor_hits += score.distractor_hits
                reduction = 1 - score.lines / generated.line_count
                compactions.append(
                    min(1.0, max(0.0, reduction) / COMPACTION_SATURATION)
                    if score.preserved
                    else 0.0
                )

    instruction_hits = sum(
        1 for _, check in INSTRUCTION_CHECKS if check(single_system, map_system)
    )
    components = {
        "term_f1": terms.f1(),
        "name_f1": names.f1(),
        "cluster_f1": clusters.f1(),
        "speaker_score": max(0.0, speaker_points / speaker_labels),
        "boundary_f1": boundaries.f1(),
        "compaction": sum(compactions) / len(compactions),
        "instructions": instruction_hits / len(INSTRUCTION_CHECKS),
    }
    total = sum(WEIGHTS[name] * value for name, value in components.items())
    if leaked:
        print(
            f"overfitting guard: worker source gained corpus words {leaked}",
            file=sys.stderr,
        )
        total = 0.0
    print(f"METRIC correction_score={total:.2f}")
    for name, value in components.items():
        print(f"METRIC {name}={value:.4f}")
    print(f"METRIC false_hints={terms.fp + names.fp + clusters.fp + boundaries.fp}")
    print(f"METRIC distractor_hits={distractor_hits}")
    print(f"METRIC guard_leaks={len(leaked)}")
    print(f"METRIC request_chars={request_chars // len(SEEDS)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
