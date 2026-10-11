"""Deterministic correction evidence for the refinement request.

Korean ASR misspells unfamiliar words in predictable ways (jamo confusions,
dropped or added spaces) and diarization mislabels short replies and the
tails of sentences. A general LLM fixes these far more reliably when the
request points at the suspicious spans than when it is asked to rewrite the
transcript on its own. This module derives that evidence only from what the
user supplied (glossary, background, roster) and the transcript itself; it
never edits the transcript's words. The model makes the final call.
"""

import os
import re
from collections import Counter
from collections.abc import Iterable
from dataclasses import dataclass
from functools import lru_cache
from itertools import pairwise
from typing import TYPE_CHECKING, Final

if TYPE_CHECKING:
    from .minutes_prompt import GlossaryEntry, Participant

LINE: Final = re.compile(r"^\[(SPEAKER_\d+|UNKNOWN)\]\s*\((\d+)s\)\s*(.*)$")
QUOTED: Final = re.compile(r'"([^"]+)"')
TIMESTAMP: Final = re.compile(r"\((\d+)s\)")
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
        "ㅗㅜ",
        "ㅛㅠ",
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
    "마다",
    "조차",
    "마저",
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
# Hesitations and discourse markers: a line that ends or starts with one is
# a speaker trailing off or taking a turn, not a sentence cut in half.
FILLERS: Final = frozenset(
    (
        *("음", "어", "응", "아", "에", "으", "뭐", "그", "저", "막", "좀", "이제"),
        *("그냥", "약간", "진짜", "근데", "그래서", "그리고", "그러니까", "그니까"),
        *("아니", "또", "이렇게", "그럼", "그러면"),
    )
)
# Pronouns that open a new sentence (`그게 문제예요`) but, at the end of a
# line, are a subject still waiting for its predicate.
OPENERS: Final = frozenset(
    ("그런", "이런", "저런", "그거", "이게", "그게", "이거", "저거")
)
# Nouns that only follow another word (`할 것`, `이런 거`); a space before
# them is grammar, not a split word.
BOUND_NOUNS: Final = frozenset(("것", "거", "걸", "게", "수", "줄", "데", "때", "뿐"))
# Agreement said as its own turn.
BACKCHANNEL: Final = re.compile(
    r"^(그치|그렇지|그죠|맞아|맞어|그러니까|그니까)[,.!?\s]"
)
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
# Without punctuation only endings that never close a noun count; `지`, `네`,
# and `군` also end nouns such as `휴지`, and a bare `다` ends `둘 다`, so `다`
# needs a verb before it.
SENTENCE_END: Final = re.compile(
    r"[.?!…]$|(요|죠|까|니다|[었았였했겠있없같이한된는]다)$"
)
# Inflected verbs and adjectives: their spellings differ only by ending and
# are never one misheard word.
PREDICATE_END: Final = re.compile(
    r"(요|다|면|까|죠|데|며|니|게|야|고|서|가|간|할|된|한|난|던|은|는|을|지만|잖아"
    r"|거든|더라|었어|았어|였어|겠어|느냐|구나)$"
)
SELF_INTRO: Final = (
    re.compile(r"(?:^|\s)저(?:는)?\s+(\S+?)(?:이|가)?\s"),
    re.compile(r"(\S+?)(?:입니다|이고요|이고|인데요|이에요)(?:[.,\s]|$)"),
)
TERM_THRESHOLD: Final = 0.2
# A span of as many words as the term with one word exactly right may differ
# more in the other (`{오인식} 차트` for `{용어} 차트`).
ALIGNED_TERM_THRESHOLD: Final = 0.25
ONCE_THRESHOLD: Final = 0.12
INTRO_SUFFIX: Final = re.compile(r"(입니다|이고요|이고|인데요|이에요)[.,]?$")
NAME_THRESHOLD: Final = 0.17
# Choosing which roster member was addressed only competes among the roster,
# so it tolerates one more confusion than flagging a name as misheard.
ADDRESS_THRESHOLD: Final = 0.25
# Two listed names this close can be written for each other by recognition
# biased toward the roster (one syllable apart, or two near-homophones).
SIMILAR_NAME_THRESHOLD: Final = 0.34
# Handovers to the shared label each name needs before the pair is reported;
# one mention apiece is often a passing reference followed by the same reply.
SIMILAR_NAME_EVIDENCE: Final = 2
# Grouping has no registered spelling to anchor it, and two ordinary
# compounds can sit one consonant apart, so it is stricter than matching.
CLUSTER_THRESHOLD: Final = 0.15
CLUSTER_TERM_THRESHOLD: Final = 0.25
CLUSTER_TERM_MAX: Final = 0.35
SILENT_INITIAL: Final = 11  # ㅇ
# Loanword spelling writes a final consonant with its representative letter
# (`북`, not `붘`).
LOANWORD_FINAL: Final = {
    "ㅋ": "ㄱ",
    "ㄲ": "ㄱ",
    "ㅍ": "ㅂ",
    "ㅌ": "ㅅ",
    "ㄷ": "ㅅ",
    "ㅆ": "ㅅ",
}
VERB_FORM: Final = re.compile(r"(다|요|된|한|할|하는|되는|해서|하고|되어|했던|됐던)$")
BACKGROUND_TOKEN: Final = re.compile(r"[가-힣A-Za-z0-9]{2,}")
# What may follow a noun inside one written word: a bound noun, then a
# particle or a light verb ending.
WORD_TAIL: Final = re.compile(
    r"(별|들|쪽|용|측|상|적|간|화)?"
    r"((하|되|해|했|돼|됐|시키|시켜)[가-힣]{0,4}|한|할|함|된|될|됨|도|을|를|은|는|이|가|에|의|로|으로|과|와|만"
    r"|까지|부터|에서|이나|나|랑|이랑|에는|으로는|이라|라|마다|조차|마저)?"
)
FRAGMENT_THRESHOLD: Final = 0.34
# A sentence picked up within this many seconds is the same sentence.
CONTINUATION_SECONDS: Final = 6
# How many lines before a question to look for the person it was asked of.
ANSWERER_LOOKBACK: Final = 4
# Lines shorter than this that end on a filler are a speaker trailing off.
TRAILING_OFF_WORDS: Final = 6
# Korean numerals read out syllable by syllable.
NUMERAL: Final = re.compile(
    r"제?[영일이삼사오육륙칠팔구십백천만억조공]+"
    r"(년대|년도|년|월|일|달|초|분|시간|시|주|개|명|원|번|항|호|층|차|세|살|대|등|위|회|권|장)?"
)
# A stem followed by this many different syllables across a meeting conjugates.
INFLECTION_ENDINGS: Final = 3


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


Syllable = list[str]  # [initial, medial, final]; a non-Hangul character alone


@lru_cache(maxsize=65536)
def jamo_forms(text: str) -> tuple[tuple[str, ...], ...]:
    """A span's jamo as ASR may render it, ignoring spaces.

    Three forms, compared form by form: as spelled; as pronounced, where a
    final consonant before ㅇ moves to the next syllable (`음악` sounds like
    `으막`); and with a loanword's inserted ㅡ syllable folded back into the
    previous final (`{어}크` written for `{억}`).
    """

    spelled: list[Syllable] = []
    for character in text.lower():
        code = ord(character) - HANGUL_BASE
        if 0 <= code < 11172:
            spelled.append(
                [INITIALS[code // 588], MEDIALS[(code % 588) // 28], FINALS[code % 28]]
            )
        elif not character.isspace():
            spelled.append([character])
    pronounced = [list(syllable) for syllable in spelled]
    for current, following in pairwise(pronounced):
        if (
            len(current) == 3
            and len(following) == 3
            and following[0] == "ㅇ"
            and current[2] in INITIALS
            and current[2] != "ㅇ"
        ):
            following[0] = "ㅇ" if current[2] == "ㅎ" else current[2]
            current[2] = "·"
    folded: list[Syllable] = []
    for syllable in spelled:
        previous = folded[-1] if folded else None
        if (
            previous is not None
            and len(previous) == 3
            and previous[2] == "·"
            and syllable[1:] == ["ㅡ", "·"]
            and syllable[0] in FINALS
        ):
            final = LOANWORD_FINAL.get(syllable[0], syllable[0])
            folded[-1] = [*previous[:2], final]
        else:
            folded.append(list(syllable))
    return tuple(
        tuple(unit for syllable in form for unit in syllable)
        for form in (spelled, pronounced, folded)
    )


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
    # a silent final e lengthens the vowel before it (frame 프레임, side 사이드)
    (r"a(?=[^aeiouy]e\b)", "ei"),
    (r"i(?=[^aeiouy]e\b)", "ai"),
    (r"(?<=[^aeiouy])e\b", ""),
    (r"a[iy]", "ei"),
    (r"e[ae]", "i"),
    (r"[eo]r\b", "o"),
    (r"ti(?=on)", "s"),
    (r"ph", "p"),
    (r"th", "s"),
    (r"wh", "w"),
    (r"sh", "s"),
    (r"ch", "j"),
    (r"ck", "k"),
    (r"qu", "kw"),
    (r"ng", "n"),
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
LATIN_THRESHOLD: Final = 0.15


def spell_digit(match: re.Match[str]) -> str:
    return DIGIT_WORDS[int(match.group())]


def latin_sound(text: str) -> str:
    sound = re.sub(r"\d", spell_digit, text.lower())
    for pattern, replacement in LATIN_RULES:
        sound = re.sub(pattern, replacement, sound)
    return re.sub(r"[^a-z]", "", sound)


def latin_sounds(text: str) -> set[str]:
    """Readings of a Latin spelling; `au` is `아우` in some words (Tauri) and
    `오` in others (Claude)."""

    return {latin_sound(text), latin_sound(re.sub(r"au|aw", "o", text.lower()))}


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


def term_sounds(term: str) -> set[str]:
    """How a Latin term sounds; short all-caps terms are spelled out."""

    if term.isupper() and term.isalpha() and len(term) <= 6:
        return {hangul_sound("".join(LETTER_NAMES[letter] for letter in term))}
    return latin_sounds(term)


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
    said = hangul_sound(spoken)
    best: float | None = None
    for target in term_sounds(term):
        longest = max(len(said), len(target))
        if not target or abs(len(said) - len(target)) > longest * 0.4:
            continue
        score = sound_distance(said, target)
        consonants = sum(1 for sound in target if sound not in VOWELS)
        # Short words collide with ordinary Korean too easily to allow slack.
        if (score == 0 or (consonants >= 3 and score <= LATIN_THRESHOLD)) and (
            best is None or score < best
        ):
            best = score
    return best


@lru_cache(maxsize=65536)
def jamo_counts(text: str) -> tuple[tuple[int, dict[str, int]], ...]:
    """(length, jamo counts) of each form, for `close`'s lower bound."""

    return tuple((len(form), dict(Counter(form))) for form in jamo_forms(text))


def close(left: str, right: str, limit: float) -> float | None:
    """`distance` when it is within `limit`, else None.

    Most pairs differ in many jamo, which a count comparison proves without
    running the edit distance: every unmatched jamo costs at least half an
    edit.
    """

    for (left_size, a), (right_size, b) in zip(
        jamo_counts(left), jamo_counts(right), strict=True
    ):
        budget = limit * max(left_size, right_size)
        if abs(left_size - right_size) > budget:
            continue
        shared = sum(min(count, b.get(unit, 0)) for unit, count in a.items())
        only_left, only_right = left_size - shared, right_size - shared
        if 0.5 * min(only_left, only_right) + abs(only_left - only_right) <= budget:
            break
    else:
        return None
    score = distance(left, right)
    return score if score <= limit else None


@lru_cache(maxsize=262144)
def distance(left: str, right: str) -> float:
    """Confusion-weighted edit distance between the closest forms of two
    spans, normalized by the longer one."""

    return min(
        edit_distance(a, b)
        for a, b in zip(jamo_forms(left), jamo_forms(right), strict=True)
    )


def edit_distance(a: tuple[str, ...], b: tuple[str, ...]) -> float:
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
            terms[term] = TERM_THRESHOLD if named else ONCE_THRESHOLD
    for entry in glossary:
        if syllables(entry.term) >= 2:
            terms[entry.term] = TERM_THRESHOLD
    known = frozenset([*terms, *names, *background_terms])
    return Vocabulary(terms, names, known)


def spoken_words(spoken: str) -> list[str]:
    return [word.strip(",.?!") for word in spoken.split()]


def plausible_span(spoken: str) -> bool:
    """Whether a span can be a misheard term at all: a filler, a pronoun, or
    a conjugated verb inside it means it is ordinary speech."""

    words = spoken_words(spoken)
    return not any(word in FILLERS or word in OPENERS for word in words)


def aligned_threshold(spoken: str, term: str, threshold: float) -> float:
    said, registered = spoken_words(spoken), term.split()
    if len(said) == len(registered) > 1 and set(said) & set(registered):
        return max(threshold, ALIGNED_TERM_THRESHOLD)
    return threshold


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
    # the term's first words without spaces (`커넥션` for `커넥션 풀`)
    leading_words: frozenset[str]

    @classmethod
    def of(cls, spelling: str, threshold: float) -> "RegisteredTerm":
        words = spelling.split()
        leading = frozenset(compact(" ".join(words[:k])) for k in range(1, len(words)))
        return cls(spelling, compact(spelling), threshold, leading)


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
                entry = RegisteredTerm.of(term, threshold)
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
        if not plausible_span(spoken):
            return found
        verbal = any(PREDICATE_END.search(word) for word in spoken_words(spoken))
        if HANGUL.search(spoken) is not None and not verbal:
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
                # partial mention of it, is not a misrecognition; neither is
                # its first words said with a particle (`{단어}을` for
                # `{단어} 풀`).
                bare = compact(split_particle(spoken)[0])
                if (
                    term.compact in said
                    or term.compact.startswith(said)
                    or bare in term.leading_words
                    or said in self._known
                    or (fragment and not fragment_fits(spoken, term.spelling))
                ):
                    continue
                limit = aligned_threshold(spoken, term.spelling, term.threshold)
                score = close(spoken, term.spelling, limit)
                if score is not None:
                    found.append((score, term.spelling))
        return found


def fragment_fits(spoken: str, term: str) -> bool:
    """A one-syllable last piece counts only against a matching last word.

    `{단어} 플` may be `{단어} 풀`, but `{단어} 사` cut from a longer word
    says nothing about it.
    """

    said, registered = spoken.split(), term.split()
    return (
        len(said) == len(registered)
        and close(said[-1], registered[-1], FRAGMENT_THRESHOLD) is not None
    )


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
            # The whole word is a span too: a term may end in a syllable
            # that doubles as a particle (`{용어}이` read as `{용어}` + 이).
            spans = [(" ".join([*head, stem]), False), (" ".join([*head, last]), False)]
            spans += [
                (" ".join([*head, last[:cut]]), cut < 2)
                for cut in range(1, len(last))
                # `으` belongs to the particle `으로` that follows it
                if WORD_TAIL.fullmatch(last[cut:]) and not last[:cut].endswith("으")
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
    """Misheard terms and names; a repeated misspelling says how often."""

    seen: Counter[tuple[str, str, str]] = Counter()
    matcher = TermMatcher(vocabulary)
    for line in lines:
        words = line.text.split()
        used: set[int] = set()
        for _, start, end, spoken, term in sorted(term_candidates(words, matcher)):
            if used.isdisjoint(range(start, end)):
                used.update(range(start, end))
                seen[("용어", spoken, term)] += 1
        for index, stem, honorific in name_mentions(words):
            if index in used or stem in vocabulary.names or syllables(stem) < 2:
                continue
            # `{X}님` is certainly a person; a bare word may be ordinary prose.
            limit = ADDRESS_THRESHOLD if honorific else NAME_THRESHOLD
            match = best_match(stem, vocabulary.names, limit)
            if match is not None:
                seen[("이름", stem + honorific, match[0] + honorific)] += 1
    ordered = sorted(seen.items(), key=lambda item: (item[0][0], item[0][2], -item[1]))
    return [
        f'- {kind}: "{spoken}" → "{correct}"' + (f" ({count}회)" if count > 1 else "")
        for (kind, spoken, correct), count in ordered
    ]


def noun_stem(word: str, shortest: int = 2) -> str:
    """The word without what may follow a noun inside it (`별`, particles,
    a light verb ending), keeping at least `shortest` syllables; the
    longest such tail is removed."""

    stripped = word.rstrip(".,?!")
    stem, _ = split_particle(stripped)
    for cut in range(shortest, len(stripped)):
        if WORD_TAIL.fullmatch(stripped[cut:]):
            return stripped[:cut] if cut < len(stem) else stem
    return stem


@lru_cache(maxsize=65536)
def syllable_keys(text: str) -> frozenset[tuple[str, str]] | None:
    """(initial, vowel) of every syllable, as spelled and as pronounced.

    Two spellings within `CLUSTER_THRESHOLD` share at least one: changing a
    syllable's initial or vowel costs at least half an edit, and the budget
    is under half an edit per syllable (three jamo × the threshold). Text
    with anything but Hangul is outside that bound, so it has no keys.
    """

    characters = [c for c in text if not c.isspace()]
    if not characters or any(not "가" <= c <= "힣" for c in characters):
        return None
    keys: set[tuple[str, str]] = set()
    carried = ""
    for character in characters:
        code = ord(character) - HANGUL_BASE
        initial, medial = INITIALS[code // 588], MEDIALS[(code % 588) // 28]
        keys.add((initial, medial))
        if carried and initial == "ㅇ":
            keys.add((carried, medial))
        final = FINALS[code % 28]
        carried = final if final in INITIALS and final not in ("ㅇ", "ㅎ") else ""
    return frozenset(keys)


class GroupIndex:
    """Which spelling groups can hold a word, by the syllables they share."""

    def __init__(self) -> None:
        self._groups: dict[tuple[str, str], set[int]] = {}
        # groups with a member the bound does not cover are always compared
        self._unkeyed: set[int] = set()

    def add(self, member: str, number: int) -> None:
        keys = syllable_keys(member)
        if keys is None:
            self._unkeyed.add(number)
            return
        for key in keys:
            self._groups.setdefault(key, set()).add(number)

    def candidates(self, word: str, count: int) -> list[int]:
        """Group numbers in creation order; every group when the word has no keys."""

        keys = syllable_keys(word)
        if keys is None:
            return list(range(count))
        found = {n for key in keys for n in self._groups.get(key, ())}
        return sorted(found | self._unkeyed)


def inflections(group: set[str], word_types: set[str]) -> bool:
    """Whether a group is one stem with different endings (`그렇기`, `그렇지`).

    When the spellings differ only in their last syllable and the stem they
    share takes three or more different next syllables across the meeting,
    it is a word that conjugates, not a word that was misheard.
    """

    spellings = [compact(member) for member in group]
    stem = os.path.commonprefix(spellings)
    if len(stem) < 2 or any(len(s) - len(stem) > 1 for s in spellings):
        return False
    continuations = {
        w[len(stem)] for w in word_types if w.startswith(stem) and len(w) > len(stem)
    }
    return len(continuations) >= INFLECTION_ENDINGS


def cluster_hints(lines: list[TranscriptLine], vocabulary: Vocabulary) -> list[str]:
    """Group spellings that are probably one unlisted word."""

    counts: dict[str, int] = {}
    words = [word.strip(",.?!") for line in lines for word in line.text.split()]
    for word in words:
        stem = noun_stem(word)
        if (
            syllables(stem) >= 3
            and re.fullmatch(r"[가-힣]+", stem)
            and PREDICATE_END.search(stem) is None
            and not stem.endswith(HONORIFICS)
            # numbers said out loud (`이천십사`) differ by a digit, not a sound
            and NUMERAL.fullmatch(stem) is None
            # a one-syllable pronoun with a particle (`걔한테`) is not a word
            and not any(
                stem.endswith(p) and syllables(stem) - len(p) == 1
                for p in PARTICLES
                if len(p) >= 2
            )
        ):
            counts[stem] = counts.get(stem, 0) + 1
    word_types = {compact(word) for word in words}
    spaced_counts: dict[str, int] = {}
    # Any word of a registered multi-word term is that term, said or
    # misheard, never an unlisted word.
    parts = {
        part
        for term in vocabulary.terms
        if " " in term
        for part in term.split()
        if syllables(part) >= 2
    }
    by_length: dict[int, list[str]] = {}
    for known in vocabulary.known:
        by_length.setdefault(syllables(known), []).append(known)

    def near_known(stem: str) -> bool:
        size = syllables(stem)
        return any(
            close(stem, known, TERM_THRESHOLD) is not None
            for length in (size - 1, size, size + 1)
            for known in by_length.get(length, ())
        )

    latin_terms = [term for term in vocabulary.terms if HANGUL.search(term) is None]
    stems = [
        stem
        for stem in counts
        if not near_known(stem)
        and not any(
            close(stem, part, CLUSTER_TERM_THRESHOLD) is not None for part in parts
        )
        # a registered Latin term said aloud is a term error, not jargon
        and not any(latin_match(stem, term) is not None for term in latin_terms)
    ]
    groups: list[set[str]] = []
    index = GroupIndex()
    for stem in stems:
        for number in index.candidates(stem, len(groups)):
            group = groups[number]
            if any(
                stem != member
                and close(stem, member, CLUSTER_THRESHOLD) is not None
                and stem not in member
                and member not in stem
                for member in group
            ):
                group.add(stem)
                index.add(stem, number)
                break
        else:
            groups.append({stem})
            index.add(stem, len(groups) - 1)
    # An unlisted word split by a space joins the group its joined spelling
    # belongs to; neither half may be a spelling of its own (`그 {단어}`). A
    # word seen once needs a different spelling, not just a space, so an
    # ordinary spacing slip is not reported as jargon.
    sizes = {syllables(member) for group in groups for member in group}
    placed: set[str] = set()
    for line in lines:
        for first, second in pairwise(line.text.split()):
            if first.endswith((",", ".", "?", "!")):
                continue
            tail = noun_stem(second, shortest=1)
            joined = first + tail
            size = syllables(joined)
            if size < 3 or not sizes & {size - 1, size, size + 1}:
                continue
            if joined in vocabulary.known or NUMERAL.fullmatch(joined):
                continue
            spaced = f"{first} {tail}"
            spaced_counts[spaced] = spaced_counts.get(spaced, 0) + 1
            if spaced in placed:
                continue
            placed.add(spaced)
            # A filler or pronoun next to a word is speech, not a split word,
            # unless removing the space gives a spelling already in the group;
            # so is a half that is already a whole word with a particle or an
            # ending (`{명사}이 잘`, `들어 가고`).
            spoken_aside = bool(
                {first, tail} & (FILLERS | OPENERS | BOUND_NOUNS)
            ) or bool(
                split_particle(first)[1]
                or PREDICATE_END.search(first)
                or PREDICATE_END.search(tail)
            )
            for number in index.candidates(joined, len(groups)):
                group = groups[number]
                if first in group or tail in group:
                    continue
                if any(
                    close(joined, member, CLUSTER_THRESHOLD) is not None
                    and (len(group) > 1 or joined != member)
                    and (not spoken_aside or joined == member)
                    for member in group
                ):
                    group.add(spaced)
                    index.add(spaced, number)
                    break
    hints: list[str] = []
    for group in groups:
        if len(group) < 2 or inflections(group, word_types):
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
        elif (
            len(group) > 2
            or sum(counts.get(m, 0) + spaced_counts.get(m, 0) for m in group) >= 3
        ):
            # Two spellings said once each are as likely two different words.
            hints.append(
                "- 같은 표현: " + " = ".join(f'"{member}"' for member in sorted(group))
            )
    return hints


def speaker_hints(
    lines: list[TranscriptLine], vocabulary: Vocabulary, roster_size: int
) -> list[str]:
    votes: dict[str, dict[str, int]] = {}
    evidence: dict[tuple[str, str], list[str]] = {}

    def vote(label: str, name: str, why: str) -> None:
        if label != "UNKNOWN":
            votes.setdefault(label, {}).setdefault(name, 0)
            votes[label][name] += 1
            evidence.setdefault((label, name), []).append(why)

    def roster_name(word: str) -> str | None:
        stem, _ = split_honorific(word)
        if stem in vocabulary.names:
            return vocabulary.names[stem]
        match = best_match(stem, vocabulary.names, ADDRESS_THRESHOLD)
        return vocabulary.names[match[0]] if match is not None else None

    for index, line in enumerate(lines):
        for pattern in SELF_INTRO:
            for found in pattern.finditer(line.text + " "):
                said = found.group(1).strip(",.?!")
                # `저는 수영을` is a topic, not a name: a name in an
                # introduction carries no case particle.
                if split_particle(said)[1] and not said.endswith(("이", "가")):
                    continue
                name = roster_name(said)
                if name is not None:
                    vote(line.label, name, f"{line.second}s 자기소개")
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
            # After a reply ("네, {이름}님") the address is plain even when the
            # segment cut off its punctuation.
            closing = is_reply or words[tail].endswith(",")
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
                    vote(previous.label, name, f"{line.second}s 응답 호칭")
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
                    vote(following.label, name, f"{line.second}s 호명")

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
        evidence[(remaining_labels[0], remaining_names[0])] = [
            "나머지 화자와 명단 대조"
        ]
    return [
        f"- 화자: {label} → {name} (근거: {', '.join(evidence[(label, name)][:3])})"
        for label, name in sorted(assigned.items())
    ]


def similar_name_hints(
    lines: list[TranscriptLine], vocabulary: Vocabulary
) -> list[str]:
    """Two listed names that hand the floor to the same speaker.

    Recognition biased toward the roster writes a name as its listed
    look-alike: in a real meeting with a near-namesake on the roster, half the
    mentions of one attendee came out as the other name. A roster name is
    never "misheard" for the lexical check, so the evidence is who speaks
    next: in a moderated meeting, naming someone with a title hands them the
    floor, and both names kept handing it to the same label.
    """

    pointed: dict[str, Counter[str]] = {}
    for index, line in enumerate(lines):
        following = next(
            (l.label for l in lines[index + 1 :] if l.label != line.label), None
        )
        if following is None or following == "UNKNOWN":
            continue
        named = {
            vocabulary.names[stem]
            for _, stem, _ in name_mentions(spoken_words(line.text))
            if stem in vocabulary.names
        }
        # A line naming several people (introductions, summaries) hands the
        # floor to at most one of them.
        if len(named) == 1:
            pointed.setdefault(named.pop(), Counter())[following] += 1
    text = " ".join(line.text for line in lines)
    dominant = {name: tally.most_common(1)[0][0] for name, tally in pointed.items()}
    found: list[str] = []
    names = sorted(pointed, key=lambda name: (-sum(pointed[name].values()), name))
    for index, first in enumerate(names):
        for second in names[index + 1 :]:
            if close(first, second, SIMILAR_NAME_THRESHOLD) is None:
                continue
            label = dominant[first]
            # A label most names hand over to is a host or chair answering
            # everyone, not evidence about who either name is.
            if (
                dominant[second] != label
                or min(pointed[first][label], pointed[second][label])
                < SIMILAR_NAME_EVIDENCE
                or any(
                    owner == label and name not in (first, second)
                    for name, owner in dominant.items()
                )
            ):
                continue
            found.append(
                f'- 비슷한 이름: "{first}"({text.count(first)}회) ↔ '
                f'"{second}"({text.count(second)}회) (근거: 두 이름 모두 부른 뒤 '
                f"{label}이 말함. 한 사람을 다른 이름으로 적었거나 화자분리가 두 "
                "사람을 한 화자로 묶었을 수 있음)"
            )
    return found


def boundary_hints(lines: list[TranscriptLine]) -> tuple[list[str], set[int]]:
    hints: list[str] = []
    flagged: set[int] = set()
    for index in range(1, len(lines)):
        line, previous = lines[index], lines[index - 1]
        following = lines[index + 1] if index + 1 < len(lines) else None
        if (
            REPLY.match(line.text)
            and len(line.text) <= 12
            # A hesitation (음, 어) after one's own question is not an answer.
            and line.text.split()[0].strip(",.?!") not in FILLERS
        ):
            asked = previous.text.endswith("?")
            # A question whose own label is already in doubt (its tail was cut
            # onto the replier) says nothing about who replied.
            misattributed = line.label == "UNKNOWN" or (
                asked and line.label == previous.label and index - 1 not in flagged
            )
            replier, reason = answerer(lines, index)
            if misattributed and replier is not None:
                hints.append(
                    f"- 화자 경계: ({line.second}s) {line.label} → {replier}"
                    f" (근거: {reason})"
                )
                flagged.add(index)
            continue
        if cut_off_tail(previous, line, following):
            hints.append(
                f"- 화자 경계: ({line.second}s) {line.label} → {previous.label}"
                f" (근거: {previous.second}s 줄이 문장 중간에서 끊김)"
            )
            flagged.add(index)
    return hints, flagged


def answerer(lines: list[TranscriptLine], index: int) -> tuple[str | None, str]:
    """Who most likely said the short reply at `index`, and why.

    The next speaker usually answered. A question is also often put to
    whoever spoke just before it, which on real meetings was about as often
    right, so when the two differ the reason names the other candidate; when
    the asker talks again right after the reply, the earlier speaker is the
    only candidate.
    """

    line, previous = lines[index], lines[index - 1]
    following = lines[index + 1] if index + 1 < len(lines) else None
    asker = previous.label
    if line.label == "UNKNOWN":
        reason = "라벨 없는 짧은 응답"
        earlier = None
    else:
        reason = f"{previous.second}s 질문자 라벨이 붙은 짧은 응답"
        earlier = next(
            (
                other
                for other in reversed(
                    lines[max(0, index - 1 - ANSWERER_LOOKBACK) : index - 1]
                )
                if other.label not in (asker, "UNKNOWN")
            ),
            None,
        )
    if following is not None and following.label not in (line.label, "UNKNOWN", asker):
        if earlier is not None and earlier.label != following.label:
            reason += f", 질문 직전 상대 화자 {earlier.label}일 수도 있음"
        return following.label, reason
    if earlier is not None:
        return earlier.label, f"{reason}, 질문 직전 상대 화자({earlier.second}s)로 추정"
    return None, reason


def cut_off_tail(
    previous: TranscriptLine, line: TranscriptLine, following: TranscriptLine | None
) -> bool:
    """Whether `line` reads as the end of `previous`'s sentence under the wrong label.

    Conversation is full of unfinished lines that are not diarization errors:
    a speaker trails off on a filler (`그`, `이제`), or the other person
    interjects and the first speaker takes the floor back. A misattributed
    tail instead finishes the cut sentence, and the label it got keeps the
    floor afterwards.
    """

    if (
        line.label == previous.label
        or previous.label == "UNKNOWN"
        or line.second - previous.second > CONTINUATION_SECONDS
        or SENTENCE_END.search(previous.text) is not None
        or SENTENCE_END.search(line.text) is None
    ):
        return False
    cut, tail = previous.text.split(), line.text.split()
    if len(cut) < 2 or not tail or cut[-1].endswith(("-", "…")):
        return False
    opening = tail[0].strip(",.?!")
    # A short line ending on a filler is someone trailing off; a long one
    # that stops on `막` or `좀` mid-phrase was cut.
    trailing_off = cut[-1].strip(",.?!") in FILLERS and len(cut) < TRAILING_OFF_WORDS
    if trailing_off or opening in FILLERS or opening in OPENERS:
        return False
    if REPLY.match(line.text) or BACKCHANNEL.match(line.text):
        return False
    return following is None or following.label != previous.label


def merge_lines(lines: list[TranscriptLine], keep: set[int]) -> list[TranscriptLine]:
    """Merge consecutive lines of one speaker.

    A flagged line never joins the line before it, so its timestamp stays a
    line timestamp the candidate can point at. Lines after it may join it,
    behind their own inline timestamp so the flagged part keeps its extent.
    """

    merged: list[TranscriptLine] = []
    flagged_head = False
    for index, line in enumerate(lines):
        if merged and index not in keep and merged[-1].label == line.label:
            previous = merged[-1]
            text = f"({line.second}s) {line.text}" if flagged_head else line.text
            merged[-1] = TranscriptLine(
                previous.label, previous.second, f"{previous.text} {text}"
            )
            flagged_head = False
        else:
            merged.append(line)
            flagged_head = index in keep
    return merged


def render_lines(lines: list[TranscriptLine]) -> str:
    return "".join(f"[{line.label}] ({line.second}s) {line.text}\n" for line in lines)


def join_sentences(lines: list[TranscriptLine]) -> list[TranscriptLine]:
    """Read an unfinished line on into the next one that follows at once."""

    joined: list[TranscriptLine] = []
    for line in lines:
        if joined:
            previous = joined[-1]
            unfinished = SENTENCE_END.search(previous.text) is None
            if unfinished and line.second - previous.second <= CONTINUATION_SECONDS:
                joined[-1] = TranscriptLine(
                    previous.label, line.second, f"{previous.text} {line.text}"
                )
                continue
        joined.append(line)
    return joined


def relevant_hints(block: str, text: str) -> str:
    """The candidates a transcript excerpt can use.

    Speaker mappings hold for the whole meeting; a spelling, grouping, or
    boundary candidate only matters where its words or timestamp appear.
    """

    kept: list[str] = []
    for line in block.splitlines():
        quoted = QUOTED.findall(line)
        if line.startswith("- 화자:"):
            kept.append(line)
        elif line.startswith("- 화자 경계:"):
            second = TIMESTAMP.search(line)
            if second is not None and f"({second.group(1)}s)" in text:
                kept.append(line)
        elif line.startswith(("- 같은 표현:", "- 비슷한 이름:")):
            if any(all(part in text for part in word.split()) for word in quoted):
                kept.append(line)
        elif quoted and all(part in text for part in quoted[0].split()):
            kept.append(line)
    return "\n".join(kept)


def render_corrections(block: str, trailing: str = "") -> str:
    """Wrap correction candidates for a prompt; empty when there are none."""

    if not block.strip():
        return ""
    return (
        "다음은 전사본에서 자동으로 찾은 오인식·화자 후보입니다. "
        "확정이 아니므로 전사본 문맥으로 확인하세요.\n"
        f"<보정후보>\n{block.strip()}\n</보정후보>\n{trailing}"
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
    # Words cut across segments are whole again once one speaker's lines are
    # merged and an unfinished sentence is read on into the next line, even
    # when diarization gave that line another label.
    merged = merge_lines(lines, flagged)
    sentences = join_sentences(merged)
    entries = [
        *lexical_hints(sentences, vocabulary),
        *cluster_hints(sentences, vocabulary),
        *speaker_hints(lines, vocabulary, len(participants)),
        *similar_name_hints(lines, vocabulary),
        *boundaries,
    ]
    block = "\n".join(entries)
    return TranscriptHints(block, render_lines(merged))
