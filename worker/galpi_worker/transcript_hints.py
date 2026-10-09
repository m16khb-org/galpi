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
from collections import Counter
from collections.abc import Iterable
from dataclasses import dataclass
from functools import lru_cache
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
    "이라고",
    "이라는",
    "이라서",
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
CLUSTER_TERM_THRESHOLD: Final = 0.25
CLUSTER_TERM_MAX: Final = 0.35
SILENT_INITIAL: Final = 11  # ㅇ
VERB_FORM: Final = re.compile(r"(다|요|된|한|할|하는|되는|해서|하고|되어|했던|됐던)$")
BACKGROUND_TOKEN: Final = re.compile(r"[가-힣A-Za-z0-9]{2,}")
# What may follow a noun inside one written word: a bound noun, then a
# particle or a light verb ending.
WORD_TAIL: Final = re.compile(
    r"(별|들|쪽|용|측|상|적|간|화)?"
    r"((하|되|해|했|돼|됐|시키|시켜)[가-힣]{0,4}|한|할|함|된|될|됨|도|을|를|은|는|이|가|에|의|로|으로|과|와|만"
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


@lru_cache(maxsize=65536)
def jamo_units(text: str) -> tuple[str, ...]:
    """Pronounced jamo, ignoring spaces; non-Hangul characters stay as-is.

    A final consonant before a syllable starting with ㅇ is pronounced as
    that syllable's initial (`음악` sounds like `으막`), which is how ASR
    hears it.
    """

    units: list[str] = []
    characters = [c for c in text.lower() if not c.isspace()]
    carried = ""
    for index, character in enumerate(characters):
        code = ord(character) - HANGUL_BASE
        if not 0 <= code < 11172:
            units.append(character)
            carried = ""
            continue
        initial, final = INITIALS[code // 588], FINALS[code % 28]
        if carried and initial == "ㅇ":
            initial = carried
        upcoming = (
            ord(characters[index + 1]) - HANGUL_BASE
            if index + 1 < len(characters)
            else -1
        )
        carried = ""
        silent_next = 0 <= upcoming < 11172 and upcoming // 588 == SILENT_INITIAL
        if silent_next and final in INITIALS and final != "ㅇ":
            carried = "" if final == "ㅎ" else final
            final = "·"
        units += [initial, MEDIALS[(code % 588) // 28], final]
    return tuple(units)


# Hangul and Latin spellings meet in a rough sound alphabet: consonants by
# place of articulation, vowels by quality, glides as `w`. Korean inserts ㅡ
# to break up consonant clusters and drops English post-vowel `r`, so both
# disappear.
HANGUL: Final = re.compile(r"[가-힣]")
INITIAL_SOUND: Final = dict(
    zip(
        INITIALS,
        [
            "k",
            "k",
            "n",
            "t",
            "t",
            "r",
            "m",
            "p",
            "p",
            "s",
            "s",
            "",
            "j",
            "j",
            "j",
            "k",
            "t",
            "p",
            "",
        ],
        strict=True,
    )
)
MEDIAL_SOUND: Final = dict(
    zip(
        MEDIALS,
        [
            "a",
            "e",
            "a",
            "e",
            "o",
            "e",
            "o",
            "e",
            "o",
            "wa",
            "we",
            "we",
            "o",
            "u",
            "wo",
            "we",
            "wi",
            "u",
            "",
            "i",
            "i",
        ],
        strict=True,
    )
)
FINAL_SOUND: Final = dict(
    zip(
        FINALS,
        [
            "",
            "k",
            "k",
            "k",
            "n",
            "n",
            "n",
            "t",
            "r",
            "k",
            "m",
            "r",
            "r",
            "r",
            "p",
            "r",
            "m",
            "p",
            "p",
            "t",
            "t",
            "n",
            "t",
            "t",
            "k",
            "t",
            "p",
            "",
        ],
        strict=True,
    )
)
LETTER_NAMES: Final = dict(
    zip(
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        [
            "에이",
            "비",
            "씨",
            "디",
            "이",
            "에프",
            "지",
            "에이치",
            "아이",
            "제이",
            "케이",
            "엘",
            "엠",
            "엔",
            "오",
            "피",
            "큐",
            "알",
            "에스",
            "티",
            "유",
            "브이",
            "더블유",
            "엑스",
            "와이",
            "지",
        ],
        strict=True,
    )
)
DIGIT_WORDS: Final = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
]
LATIN_RULES: Final[tuple[tuple[str, str], ...]] = (
    (r"(?<=[^aeiouy])e\b", ""),
    (r"ti(?=on)", "s"),
    (r"ph", "p"),
    (r"th", "s"),
    (r"wh", "w"),
    (r"sh", "s"),
    (r"ch", "j"),
    (r"ck", "k"),
    (r"qu", "kw"),
    (r"ng", "n"),
    (r"ee", "i"),
    (r"oo", "u"),
    (r"c(?=[eiy])", "s"),
    (r"x", "ks"),
    (r"g(?=[eiy])", "j"),
    (r"[qcg]", "k"),
    (r"[fvb]", "p"),
    (r"d", "t"),
    (r"z", "j"),
    (r"l", "r"),
    (r"h", ""),
    (r"y(?=[aeiou])", ""),
    (r"y", "i"),
    (r"w(?![aeiou])", "u"),
    (r"(?<=[aeiou])r(?![aeiou])", ""),
    (r"([^aeiou])\1+", r"\1"),
)
VOWELS: Final = frozenset("aeiou")
LATIN_THRESHOLD: Final = 0.2


def spell_digit(match: re.Match[str]) -> str:
    return DIGIT_WORDS[int(match.group())]


def latin_sound(text: str) -> str:
    sound = re.sub(r"\d", spell_digit, text.lower())
    for pattern, replacement in LATIN_RULES:
        sound = re.sub(pattern, replacement, sound)
    return re.sub(r"[^a-z]", "", sound)


def hangul_sound(text: str) -> str:
    parts: list[str] = []
    for character in text:
        code = ord(character) - HANGUL_BASE
        if 0 <= code < 11172:
            parts += [
                INITIAL_SOUND[INITIALS[code // 588]],
                MEDIAL_SOUND[MEDIALS[(code % 588) // 28]],
                FINAL_SOUND[FINALS[code % 28]],
            ]
        elif character.isalnum():
            parts.append(latin_sound(character))
    return re.sub(r"([^aeiou])\1+", r"\1", "".join(parts))


def term_sound(term: str) -> str:
    """How a Latin term sounds; short all-caps terms are spelled out."""

    if term.isupper() and term.isalpha() and len(term) <= 6:
        return hangul_sound("".join(LETTER_NAMES[letter] for letter in term))
    return latin_sound(term)


def sound_distance(a: str, b: str) -> float:
    previous = [0.0]
    for y in b:
        previous.append(previous[-1] + (0.5 if y in VOWELS else 1.0))
    for x in a:
        gap = 0.5 if x in VOWELS else 1.0
        current = [previous[0] + gap] + [0.0] * len(b)
        for j, y in enumerate(b, 1):
            if x == y:
                cost = 0.0
            elif x in VOWELS and y in VOWELS:
                cost = 0.5
            else:
                cost = 1.0
            current[j] = min(
                previous[j] + gap,
                current[j - 1] + (0.5 if y in VOWELS else 1.0),
                previous[j - 1] + cost,
            )
        previous = current
    return previous[-1] / max(len(a), len(b), 1)


@lru_cache(maxsize=262144)
def latin_match(spoken: str, term: str) -> float | None:
    """Distance from a Hangul span to a Latin term it may be the sound of."""

    if syllables(spoken) < 2:
        return None
    said, target = hangul_sound(spoken), term_sound(term)
    longest = max(len(said), len(target))
    if not target or abs(len(said) - len(target)) > longest * 0.4:
        return None
    score = sound_distance(said, target)
    consonants = sum(1 for sound in target if sound not in VOWELS)
    # Short words collide with ordinary Korean too easily to allow slack.
    if score == 0 or (consonants >= 3 and score <= LATIN_THRESHOLD):
        return score
    return None


@lru_cache(maxsize=65536)
def jamo_counts(text: str) -> Counter[str]:
    return Counter(jamo_units(text))


def close(left: str, right: str, limit: float) -> float | None:
    """`distance` when it is within `limit`, else None.

    Most pairs differ in many jamo, which a count comparison proves without
    running the edit distance: every unmatched jamo costs at least half an
    edit.
    """

    a, b = jamo_counts(left), jamo_counts(right)
    only_left, only_right = (a - b).total(), (b - a).total()
    floor = 0.5 * min(only_left, only_right) + abs(only_left - only_right)
    if floor > limit * max(a.total(), b.total()):
        return None
    score = distance(left, right)
    return score if score <= limit else None


@lru_cache(maxsize=262144)
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
        if (
            syllables(term) >= 2
            and VERB_FORM.search(term) is None
            and re.search(r"[가-힣A-Za-z]", term) is not None
        ):
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
        if span == candidate or abs(syllables(span) - syllables(candidate)) > 1:
            continue
        score = close(span, candidate, threshold)
        if score is not None and (best is None or score < best[1]):
            best = (candidate, score)
    return best


def compact(text: str) -> str:
    return text.replace(" ", "")


@dataclass(frozen=True, slots=True)
class RegisteredTerm:
    spelling: str
    compact: str
    threshold: float


class TermMatcher:
    """Registered terms indexed by length, with results memoized per span.

    Meetings repeat the same words constantly, and each span is compared
    with every registered term of a similar length, so both the index and
    the memo decide whether a two-hour transcript takes seconds or minutes.
    """

    def __init__(self, vocabulary: Vocabulary) -> None:
        self._known = frozenset(compact(word) for word in vocabulary.known)
        self._latin: list[str] = []
        self._by_length: dict[int, list[RegisteredTerm]] = {}
        for term, threshold in vocabulary.terms.items():
            if HANGUL.search(term) is None:
                self._latin.append(term)
            else:
                entry = RegisteredTerm(term, compact(term), threshold)
                self._by_length.setdefault(len(entry.compact), []).append(entry)
        self._memo: dict[tuple[str, bool], tuple[tuple[float, str], ...]] = {}

    def matches(self, spoken: str, fragment: bool) -> tuple[tuple[float, str], ...]:
        key = (spoken, fragment)
        found = self._memo.get(key)
        if found is None:
            found = self._memo[key] = tuple(self._match(spoken, fragment))
        return found

    def _match(self, spoken: str, fragment: bool) -> list[tuple[float, str]]:
        found: list[tuple[float, str]] = []
        if HANGUL.search(spoken) is not None:
            for term in self._latin:
                score = latin_match(spoken, term)
                if score is not None:
                    found.append((score, term))
        said = compact(spoken)
        for size in (len(said) - 1, len(said), len(said) + 1):
            for term in self._by_length.get(size, ()):
                if spoken == term.spelling:
                    continue
                if said == term.compact:
                    found.append((0.0, term.spelling))
                    continue
                # The registered spelling already present, or a correct
                # partial mention of it, is not a misrecognition.
                if (
                    fragment
                    or term.compact in said
                    or term.compact.startswith(said)
                    or said in self._known
                ):
                    continue
                score = close(spoken, term.spelling, term.threshold)
                if score is not None:
                    found.append((score, term.spelling))
        return found


def term_candidates(
    words: list[str], matcher: TermMatcher
) -> list[tuple[float, int, int, str, str]]:
    """(distance, start, end, spoken span, registered term) for one line.

    A span is up to three words, or the front of one word when a suffix such
    as `별` or `쪽` follows the term. Spacing-only differences count as
    distance zero but still differ from the registered spelling.
    """

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
            for spoken, fragment in spans:
                if fragment and width == 1:
                    continue
                for score, term in matcher.matches(spoken, fragment):
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
    matcher = TermMatcher(vocabulary)
    for line in lines:
        words = line.text.split()
        used: set[int] = set()
        for _, start, end, spoken, term in sorted(term_candidates(words, matcher)):
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
            close(stem, known, TERM_THRESHOLD) is not None for known in vocabulary.known
        )
        and not any(
            close(stem, part, CLUSTER_TERM_THRESHOLD) is not None for part in parts
        )
    ]
    groups: list[set[str]] = []
    for stem in stems:
        for group in groups:
            if any(
                stem != member
                and close(stem, member, CLUSTER_THRESHOLD) is not None
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
        spread = [distance(member, target) for member in group] if target else []
        if spread and (
            max(spread) <= CLUSTER_TERM_MAX
            and sum(spread) / len(spread) <= CLUSTER_TERM_THRESHOLD
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
