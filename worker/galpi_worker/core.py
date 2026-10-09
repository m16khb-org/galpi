"""Pure domain helpers for the Galpi worker."""

import re
from collections import Counter
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Literal, cast

SpeakerMode = Literal["auto", "exact", "range"]


class InvalidInput(ValueError):
    """A caller-supplied argument or payload the worker refuses to run on.

    Subclasses ValueError so existing callers keep working, but lets
    ``__main__`` tell Galpi's own validation apart from a ValueError raised
    deep inside a model library, which is an engine failure rather than bad
    input.
    """


# The model's hotwords slot keeps roughly the first 223 prompt tokens; Korean
# syllables average about one token each, so a character budget slightly under
# that leaves safe headroom for the separator and special tokens.
ASR_HOTWORDS_CHAR_BUDGET = 200


@dataclass(frozen=True, slots=True)
class SpeakerHint:
    mode: SpeakerMode
    exact: int | None = None
    minimum: int | None = None
    maximum: int | None = None


_HALLUCINATION_PATTERN = re.compile(
    r"시청해\s*주?셔서|한글자막\s*by|구독과\s*좋아요|이\s*시각\s*세계였습니다"
)

# A repetition loop is a Whisper failure mode on silence or noise: one short
# token repeats with high confidence for the whole segment (a person's name
# dozens of times), so confidence-based rules never catch it. Meeting speech
# never has one token dominating a whole segment, so token dominance is the
# tell: at least six tokens with the same token covering 80% of them.
_REPETITION_MIN_TOKENS = 6
_REPETITION_DOMINANCE = 0.8
_REPETITION_TOKEN_SPLIT = re.compile(r"[,\.\s]+")

# Token dominance only catches a single word looping. A decoder that stalls on
# a whole phrase ("네 알겠습니다 네 알겠습니다 ...") keeps every token below the
# dominance bar, so phrases are checked too: if one repeated n-gram covers most
# of the segment, nothing was actually said.
_PHRASE_MIN_TOKENS = 8
_PHRASE_LENGTHS = (2, 3, 4)
_PHRASE_COVERAGE = 0.6


def validate_speaker_hint(hint: SpeakerHint) -> None:
    """Validate speaker hints before model work starts."""

    if hint.mode == "auto":
        return
    if hint.mode == "exact":
        if hint.exact is None or hint.exact <= 0:
            raise InvalidInput("speaker count must be greater than zero")
        return
    if hint.minimum is None or hint.maximum is None:
        raise InvalidInput("speaker range requires minimum and maximum")
    if hint.minimum <= 0 or hint.maximum <= 0:
        raise InvalidInput("speaker range values must be greater than zero")
    if hint.minimum > hint.maximum:
        raise InvalidInput("speaker range minimum must not exceed maximum")


def should_filter_segment(text: str) -> bool:
    """Return whether a segment is a known hallucination or a repetition loop."""

    if _HALLUCINATION_PATTERN.search(text) is not None:
        return True
    tokens = [token for token in _REPETITION_TOKEN_SPLIT.split(text) if token]
    if len(tokens) < min(_REPETITION_MIN_TOKENS, _PHRASE_MIN_TOKENS):
        return False
    dominant = max(Counter(tokens).values())
    return _is_repetition_loop(tokens, dominant) or _is_phrase_loop(tokens, dominant)


def _is_repetition_loop(tokens: list[str], dominant: int) -> bool:
    if len(tokens) < _REPETITION_MIN_TOKENS:
        return False
    return dominant / len(tokens) >= _REPETITION_DOMINANCE


def _is_phrase_loop(tokens: list[str], dominant: int) -> bool:
    if len(tokens) < _PHRASE_MIN_TOKENS:
        return False
    for length in _PHRASE_LENGTHS:
        # A phrase repeats at most as often as its first token, so a length
        # the most frequent token cannot reach the coverage with is skipped.
        if (
            len(tokens) < length * 2
            or dominant * length / len(tokens) < _PHRASE_COVERAGE
        ):
            continue
        phrases = zip(*(tokens[offset:] for offset in range(length)), strict=False)
        repeats = max(Counter(phrases).values())
        if repeats * length / len(tokens) >= _PHRASE_COVERAGE:
            return True
    return False


def parse_asr_context(payload: object) -> tuple[list[str], list[str], list[str]]:
    """Read the biasing lists handed over by the host as (terms, names, aliases)."""

    if not isinstance(payload, dict):
        raise TypeError("asr context payload was not a JSON object")
    fields = cast(dict[str, object], payload)

    def strings(key: str) -> list[str]:
        raw = fields.get(key)
        if raw is None:
            return []
        if not isinstance(raw, list):
            raise TypeError(f"asr context {key} was not a JSON array")
        return [
            value.strip()
            for value in cast(list[object], raw)
            if isinstance(value, str) and value.strip()
        ]

    return strings("terms"), strings("names"), strings("aliases")


def build_asr_hotwords(
    terms: Sequence[str], names: Sequence[str], aliases: Sequence[str]
) -> str:
    """Pack glossary terms, participant names, and aliases into one biasing string.

    Glossary terms outrank names, names outrank aliases: rare domain words gain
    the most from biasing. The hotwords slot keeps the *front* of the string when
    the model truncates, so the first fitting entries survive and whole entries
    drop once the character budget runs out.
    """

    seen: set[str] = set()
    ordered: list[str] = []
    for word in (*terms, *names, *aliases):
        cleaned = word.strip()
        if cleaned and cleaned not in seen:
            seen.add(cleaned)
            ordered.append(cleaned)
    kept: list[str] = []
    used = 0
    for word in ordered:
        cost = len(word) + (2 if kept else 0)
        if used + cost > ASR_HOTWORDS_CHAR_BUDGET:
            break
        kept.append(word)
        used += cost
    return ", ".join(kept)
