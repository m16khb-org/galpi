"""Deterministic correction evidence for the refinement request.

Korean ASR misspells unfamiliar words in predictable ways (jamo confusions,
dropped or added spaces) and diarization mislabels short replies and the
tails of sentences. A general LLM fixes these far more reliably when the
request points at the suspicious spans than when it is asked to rewrite the
transcript on its own. This module derives that evidence only from what the
user supplied (glossary, background, roster) and the transcript itself; it
never edits the transcript's words. The model makes the final call.
"""

import re
from collections.abc import Iterable
from dataclasses import dataclass
from typing import TYPE_CHECKING, Final

if TYPE_CHECKING:
    from .minutes_prompt import GlossaryEntry, Participant

LINE: Final = re.compile(r"^\[(SPEAKER_\d+|UNKNOWN)\]\s*\((\d+)s\)\s*(.*)$")
HANGUL_BASE: Final = 0xAC00
INITIALS: Final = "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ"
MEDIALS: Final = "ㅏㅐㅑㅒㅓㅔㅕㅖㅗㅘㅙㅚㅛㅜㅝㅞㅟㅠㅡㅢㅣ"
FINALS: Final = "·ㄱㄲㄳㄴㄵㄶㄷㄹㄺㄻㄼㄽㄾㄿㅀㅁㅂㅄㅅㅆㅇㅈㅊㅋㅌㅍㅎ"
# Pairs Korean ASR confuses; substituting one for the other costs half.
CONFUSABLE: Final = frozenset(
    frozenset(pair)
    for pair in (
        "ㄱㅋ",
        "ㄱㄲ",
        "ㄷㅌ",
        "ㄷㄸ",
        "ㅂㅍ",
        "ㅂㅃ",
        "ㅈㅊ",
        "ㅈㅉ",
        "ㅅㅆ",
        "ㅇㅎ",
        "ㄴㄹ",
        "ㅐㅔ",
        "ㅒㅖ",
        "ㅓㅗ",
        "ㅡㅜ",
        "ㅕㅛ",
        "ㅙㅚ",
        "ㅚㅞ",
        "ㅢㅣ",
        "ㅟㅣ",
        "ㄴㅇ",
        "ㅁㅇ",
        "·ㄴ",
        "·ㅇ",
        "·ㄹ",
        "·ㄱ",
    )
)
PARTICLES: Final = (
    "께서는",
    "에서는",
    "으로는",
    "께서",
    "에서",
    "에게",
    "한테",
    "으로",
    "이랑",
    "라고",
    "이고",
    "이며",
    "인데",
    "처럼",
    "까지",
    "부터",
    "보다",
    "랑",
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
    "만",
)
HONORIFICS: Final = ("님", "씨")
# Job titles said between a name and its honorific ("{이름} 매니저님").
TITLES: Final = frozenset(
    [
        "매니저",
        "팀장",
        "파트장",
        "실장",
        "본부장",
        "센터장",
        "대리",
        "과장",
        "차장",
        "부장",
        "주임",
        "사원",
        "책임",
        "선임",
        "수석",
        "연구원",
        "교수",
        "박사",
        "선생",
        "대표",
        "이사",
        "상무",
        "전무",
        "리드",
        "프로",
    ]
)
REPLY: Final = re.compile(
    r"^(네|예|응|아|어|음|맞아요|맞습니다|맞아|그렇죠|그렇습니다|좋습니다|좋아요|알겠습니다)([,.!?\s]|$)"
)
SENTENCE_END: Final = re.compile(r"[.?!…]$|(요|다|죠|까|네|군|지)$")
# Inflected verbs and adjectives: their spellings differ only by ending and
# are never one misheard word.
PREDICATE_END: Final = re.compile(
    r"(요|다|면|까|죠|데|며|니|게|야|고|서|가|간|할|된|한|난|던|은|는|을|지만|잖아)$"
)
SELF_INTRO: Final = (
    re.compile(r"(?:^|\s)저(?:는)?\s+(\S+?)(?:이|가)?\s"),
    re.compile(r"(\S+?)(?:입니다|이고요|이고|인데요|이에요)(?:[.,\s]|$)"),
)
TERM_THRESHOLD: Final = 0.2
LONG_TERM_THRESHOLD: Final = 0.25
ONCE_THRESHOLD: Final = 0.12
INTRO_SUFFIX: Final = re.compile(r"(입니다|이고요|이고|인데요|이에요)[.,]?$")
NAME_THRESHOLD: Final = 0.17
CLUSTER_THRESHOLD: Final = 0.2
CLUSTER_TERM_THRESHOLD: Final = 0.3
VERB_FORM: Final = re.compile(r"(다|요|된|한|할|하는|되는|해서|하고|되어|했던|됐던)$")
BACKGROUND_TOKEN: Final = re.compile(r"[가-힣A-Za-z0-9]{2,}")
# What may follow a noun inside one written word: a bound noun, then a
# particle or a light verb ending.
WORD_TAIL: Final = re.compile(
    r"(별|들|쪽|용|측|상|적|간|화)?"
    r"(하는|하고|해서|하면|한|할|된|되는|되고|도|을|를|은|는|이|가|에|의|로|으로|과|와|만"
    r"|까지|부터|에서|이나|나|랑|이랑|에는|으로는|이라|라)?"
)


@dataclass(frozen=True, slots=True)
class TranscriptLine:
    label: str
    second: int
    text: str


@dataclass(frozen=True, slots=True)
class TranscriptHints:
    """The evidence block and the transcript to send with it."""

    block: str
    transcript: str


def parse_lines(transcript: str) -> list[TranscriptLine]:
    lines: list[TranscriptLine] = []
    for raw in transcript.splitlines():
        match = LINE.match(raw.strip())
        if match is not None:
            lines.append(
                TranscriptLine(
                    match.group(1), int(match.group(2)), match.group(3).strip()
                )
            )
    return lines


def jamo_units(text: str) -> list[str]:
    """Spelling as jamo, ignoring spaces; non-Hangul characters stay as-is."""

    units: list[str] = []
    for character in text.lower():
        code = ord(character) - HANGUL_BASE
        if 0 <= code < 11172:
            units += [
                INITIALS[code // 588],
                MEDIALS[(code % 588) // 28],
                FINALS[code % 28],
            ]
        elif not character.isspace():
            units.append(character)
    return units


def distance(left: str, right: str) -> float:
    """Confusion-weighted edit distance, normalized by the longer spelling."""

    a, b = jamo_units(left), jamo_units(right)
    if not a or not b:
        return 1.0
    previous = [float(j) for j in range(len(b) + 1)]
    for i, x in enumerate(a, 1):
        current = [float(i)] + [0.0] * len(b)
        for j, y in enumerate(b, 1):
            if x == y:
                cost = 0.0
            elif frozenset((x, y)) in CONFUSABLE:
                cost = 0.5
            else:
                cost = 1.0
            current[j] = min(
                previous[j] + 1, current[j - 1] + 1, previous[j - 1] + cost
            )
        previous = current
    return previous[-1] / max(len(a), len(b))


def split_particle(word: str) -> tuple[str, str]:
    stripped = word.rstrip(".,?!")
    for particle in PARTICLES:
        if stripped.endswith(particle) and len(stripped) > len(particle) + 1:
            return stripped[: -len(particle)], particle
    return stripped, ""


def split_honorific(word: str) -> tuple[str, str]:
    stem, _ = split_particle(word)
    for honorific in HONORIFICS:
        if stem.endswith(honorific) and len(stem) > len(honorific):
            return stem[: -len(honorific)], honorific
    return stem, ""


def syllables(text: str) -> int:
    return sum(1 for character in text if not character.isspace())


@dataclass(frozen=True, slots=True)
class Vocabulary:
    terms: dict[str, float]  # registered spelling -> accepted distance
    names: dict[str, str]  # spoken form -> roster name
    known: frozenset[str]


def build_vocabulary(
    background: str, participants: "list[Participant]", glossary: "list[GlossaryEntry]"
) -> Vocabulary:
    names: dict[str, str] = {}
    for person in participants:
        forms = [
            person.name,
            *person.aliases,
            *(alias.split()[0] for alias in person.aliases),
        ]
        # Three-syllable Korean names are often said without the surname.
        if len(person.name) == 3 and all("가" <= c <= "힣" for c in person.name):
            forms.append(person.name[1:])
        for form in forms:
            spoken, _ = split_honorific(form)
            if syllables(spoken) >= 2:
                names[spoken] = person.name
    background_terms = [
        split_particle(token)[0] for token in BACKGROUND_TOKEN.findall(background)
    ]
    counts: dict[str, int] = {}
    for term in background_terms:
        counts[term] = counts.get(term, 0) + 1
    terms: dict[str, float] = {}
    # A word the background mentions once may be ordinary prose, so it only
    # attracts near-identical spellings; repeated or Latin words are names of
    # things and get the glossary's tolerance.
    for term, count in counts.items():
        if syllables(term) >= 2 and VERB_FORM.search(term) is None:
            named = count >= 2 or re.search(r"[가-힣]", term) is None
            terms[term] = term_threshold(term) if named else ONCE_THRESHOLD
    for entry in glossary:
        if syllables(entry.term) >= 2:
            terms[entry.term] = term_threshold(entry.term)
    known = frozenset([*terms, *names, *background_terms])
    return Vocabulary(terms, names, known)


def term_threshold(term: str) -> float:
    return LONG_TERM_THRESHOLD if syllables(term) >= 4 else TERM_THRESHOLD


def best_match(
    span: str, candidates: Iterable[str], threshold: float
) -> tuple[str, float] | None:
    best: tuple[str, float] | None = None
    for candidate in candidates:
        if abs(syllables(span) - syllables(candidate)) > 1:
            continue
        score = distance(span, candidate)
        if 0 < score <= threshold and (best is None or score < best[1]):
            best = (candidate, score)
    return best


def compact(text: str) -> str:
    return text.replace(" ", "")


def term_candidates(
    words: list[str], vocabulary: Vocabulary
) -> list[tuple[float, int, int, str, str]]:
    """(distance, start, end, spoken span, registered term) for one line.

    A span is up to three words, or the front of one word when a suffix such
    as `별` or `쪽` follows the term. Spacing-only differences count as
    distance zero but still differ from the registered spelling.
    """

    known = {compact(word) for word in vocabulary.known}
    found: list[tuple[float, int, int, str, str]] = []
    for start in range(len(words)):
        for width in (1, 2, 3):
            end = start + width
            if end > len(words) or any(
                word.endswith((",", ".", "?", "!")) for word in words[start : end - 1]
            ):
                break
            head = words[start : end - 1]
            last = words[end - 1].rstrip(".,?!")
            stem, _ = split_particle(last)
            # The last word may carry a suffix (`별`, `하는`, `은`) after the
            # misheard part. A one-syllable piece is too little to judge a
            # misspelling by, so it only counts for spacing-only differences.
            spans = [(" ".join([*head, stem]), False)] + [
                (" ".join([*head, last[:cut]]), cut < 2)
                for cut in range(1, len(last))
                if WORD_TAIL.fullmatch(last[cut:])
            ]
            if width == 1:
                spans = [
                    (spoken, fragment) for spoken, fragment in spans if not fragment
                ]
            for spoken, fragment in spans:
                for term, threshold in vocabulary.terms.items():
                    said, registered = compact(spoken), compact(term)
                    if spoken == term or abs(syllables(spoken) - syllables(term)) > 1:
                        continue
                    if said == registered:
                        found.append((0.0, start, end, spoken, term))
                        continue
                    # The registered spelling already present, or a correct
                    # partial mention of it, is not a misrecognition.
                    if (
                        fragment
                        or registered in said
                        or registered.startswith(said)
                        or said in known
                    ):
                        continue
                    score = distance(spoken, term)
                    if score <= threshold:
                        found.append((score, start, end, spoken, term))
    return found


def name_mentions(words: list[str]) -> list[tuple[int, str, str]]:
    """(word index, spoken name, attached honorific) where a person is named."""

    mentions: list[tuple[int, str, str]] = []
    for index, word in enumerate(words):
        stem, honorific = split_honorific(word.rstrip(",.?!"))
        following = words[index + 1] if index + 1 < len(words) else ""
        # "{이름} 님은", "{이름} 씨가", "{이름} 연구원님"
        addressed = (
            bool(honorific)
            or following.startswith(HONORIFICS)
            or split_honorific(following.rstrip(",.?!"))[0] in TITLES
        )
        introduced = index > 0 and words[index - 1] in ("저", "저는")
        intro = INTRO_SUFFIX.search(word)
        if intro is not None and not honorific:
            mentions.append((index, word[: intro.start()], ""))
        elif addressed or introduced:
            mentions.append((index, stem, honorific))
    return mentions


def lexical_hints(lines: list[TranscriptLine], vocabulary: Vocabulary) -> list[str]:
    hints: dict[tuple[str, str], str] = {}
    for line in lines:
        words = line.text.split()
        used: set[int] = set()
        for _, start, end, spoken, term in sorted(term_candidates(words, vocabulary)):
            if used.isdisjoint(range(start, end)):
                used.update(range(start, end))
                hints[(spoken, term)] = f'- 용어: "{spoken}" → "{term}"'
        for index, stem, honorific in name_mentions(words):
            if index in used or stem in vocabulary.names or syllables(stem) < 2:
                continue
            match = best_match(stem, vocabulary.names, NAME_THRESHOLD)
            if match is None:
                continue
            surface = stem + honorific
            correct = match[0] + honorific
            hints[(surface, correct)] = f'- 이름: "{surface}" → "{correct}"'
    return list(hints.values())


def cluster_hints(lines: list[TranscriptLine], vocabulary: Vocabulary) -> list[str]:
    """Group spellings that are probably one unlisted word."""

    counts: dict[str, int] = {}
    for line in lines:
        for word in line.text.split():
            stem, _ = split_particle(word)
            if (
                syllables(stem) >= 3
                and re.fullmatch(r"[가-힣]+", stem)
                and PREDICATE_END.search(stem) is None
                and not stem.endswith(HONORIFICS)
            ):
                counts[stem] = counts.get(stem, 0) + 1
    # Any word of a registered multi-word term is that term, said or
    # misheard, never an unlisted word.
    parts = {
        part
        for term in vocabulary.terms
        if " " in term
        for part in term.split()
        if syllables(part) >= 2
    }
    stems = [
        stem
        for stem in counts
        if not any(
            distance(stem, known) <= TERM_THRESHOLD for known in vocabulary.known
        )
        and not any(distance(stem, part) <= CLUSTER_TERM_THRESHOLD for part in parts)
    ]
    groups: list[set[str]] = []
    for stem in stems:
        for group in groups:
            if any(
                0 < distance(stem, member) <= CLUSTER_THRESHOLD
                and stem not in member
                and member not in stem
                for member in group
            ):
                group.add(stem)
                break
        else:
            groups.append({stem})
    hints: list[str] = []
    for group in groups:
        if len(group) < 2:
            continue
        # Several distinct misspellings around one registered term are
        # stronger evidence than any single one: correct them all.
        target = min(
            (
                term
                for term in vocabulary.terms
                if all(abs(syllables(term) - syllables(m)) <= 1 for m in group)
            ),
            key=lambda term: max(distance(member, term) for member in group),
            default=None,
        )
        if target is not None and all(
            distance(member, target) <= CLUSTER_TERM_THRESHOLD for member in group
        ):
            hints += [f'- 용어: "{member}" → "{target}"' for member in sorted(group)]
        else:
            hints.append(
                "- 같은 표현: " + " = ".join(f'"{member}"' for member in sorted(group))
            )
    return hints


def speaker_hints(
    lines: list[TranscriptLine], vocabulary: Vocabulary, roster_size: int
) -> list[str]:
    votes: dict[str, dict[str, int]] = {}

    def vote(label: str, name: str) -> None:
        if label != "UNKNOWN":
            votes.setdefault(label, {}).setdefault(name, 0)
            votes[label][name] += 1

    def roster_name(word: str) -> str | None:
        stem, _ = split_honorific(word)
        if stem in vocabulary.names:
            return vocabulary.names[stem]
        match = best_match(stem, vocabulary.names, NAME_THRESHOLD)
        return vocabulary.names[match[0]] if match is not None else None

    for index, line in enumerate(lines):
        for pattern in SELF_INTRO:
            for found in pattern.finditer(line.text + " "):
                name = roster_name(found.group(1))
                if name is not None:
                    vote(line.label, name)
        words = line.text.split()
        # A sentence opening with an address ("{이름} 님, ...") hands the floor
        # to that person; a reply opening with one ("네, {이름}님.") answers them.
        for position, word in enumerate(words):
            prefix = words[:position]
            is_reply = (
                0 < len(prefix) <= 2 and REPLY.match(" ".join(prefix)) is not None
            )
            starts_sentence = (
                position == 0
                or is_reply
                or words[position - 1].endswith((".", "?", "!"))
            )
            if not starts_sentence:
                continue
            stem, honorific = split_honorific(word.rstrip(",.?!"))
            tail = position
            if not honorific and tail + 1 < len(words):
                title, title_honorific = split_honorific(words[tail + 1].rstrip(",.?!"))
                if title in HONORIFICS or (title in TITLES and title_honorific):
                    honorific, tail = title_honorific or title, tail + 1
            closing = words[tail].endswith(",") or (
                is_reply and words[tail].endswith((".", "!"))
            )
            if not (honorific and closing):
                continue
            name = roster_name(stem)
            if name is None:
                continue
            if is_reply:
                previous = next(
                    (
                        l
                        for l in reversed(lines[:index])
                        if l.label not in (line.label, "UNKNOWN")
                    ),
                    None,
                )
                if previous is not None:
                    vote(previous.label, name)
            else:
                following = next(
                    (
                        l
                        for l in lines[index + 1 :]
                        if l.label not in (line.label, "UNKNOWN")
                    ),
                    None,
                )
                if following is not None:
                    vote(following.label, name)

    assigned: dict[str, str] = {}
    for label, tally in sorted(votes.items(), key=lambda item: -max(item[1].values())):
        name = max(tally, key=lambda key: tally[key])
        if name not in assigned.values():
            assigned[label] = name
    labels = sorted({line.label for line in lines if line.label != "UNKNOWN"})
    remaining_labels = [label for label in labels if label not in assigned]
    remaining_names = sorted(set(vocabulary.names.values()) - set(assigned.values()))
    if (
        len(labels) == roster_size
        and len(remaining_labels) == 1
        and len(remaining_names) == 1
    ):
        assigned[remaining_labels[0]] = remaining_names[0]
    return [f"- 화자: {label} → {name}" for label, name in sorted(assigned.items())]


def boundary_hints(lines: list[TranscriptLine]) -> tuple[list[str], set[int]]:
    hints: list[str] = []
    flagged: set[int] = set()
    for index in range(1, len(lines)):
        line, previous = lines[index], lines[index - 1]
        following = lines[index + 1] if index + 1 < len(lines) else None
        if REPLY.match(line.text) and len(line.text) <= 12:
            asked = previous.text.endswith("?")
            misattributed = line.label == "UNKNOWN" or (
                asked and line.label == previous.label
            )
            if (
                misattributed
                and following is not None
                and following.label not in (line.label, "UNKNOWN")
                and following.label != previous.label
            ):
                hints.append(
                    f"- 화자 경계: ({line.second}s) {line.label} → {following.label}"
                )
                flagged.add(index)
            continue
        cut_off = SENTENCE_END.search(previous.text) is None
        if (
            cut_off
            and line.label != previous.label
            and previous.label != "UNKNOWN"
            and line.second - previous.second <= 6
        ):
            hints.append(
                f"- 화자 경계: ({line.second}s) {line.label} → {previous.label}"
            )
            flagged.add(index)
    return hints, flagged


def compact_transcript(lines: list[TranscriptLine], keep: set[int]) -> str:
    """Merge consecutive lines of one speaker, keeping flagged lines apart."""

    merged: list[list[str | int]] = []
    for index, line in enumerate(lines):
        if (
            merged
            and index not in keep
            and index - 1 not in keep
            and merged[-1][0] == line.label
        ):
            merged[-1][2] = f"{merged[-1][2]} {line.text}"
        else:
            merged.append([line.label, line.second, line.text])
    return (
        "\n".join(f"[{label}] ({second}s) {text}" for label, second, text in merged)
        + "\n"
    )


def analyze(
    transcript: str,
    background: str,
    participants: "list[Participant]",
    glossary: "list[GlossaryEntry]",
) -> TranscriptHints:
    lines = parse_lines(transcript)
    if not lines:
        return TranscriptHints("", transcript)
    vocabulary = build_vocabulary(background, participants, glossary)
    boundaries, flagged = boundary_hints(lines)
    entries = [
        *lexical_hints(lines, vocabulary),
        *cluster_hints(lines, vocabulary),
        *speaker_hints(lines, vocabulary, len(participants)),
        *boundaries,
    ]
    block = "\n".join(entries)
    return TranscriptHints(block, compact_transcript(lines, flagged))
