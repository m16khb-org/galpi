"""Deterministic benchmark of the worker's CPU-bound post-processing.

Every platform (macOS, Windows, Linux) runs this code after ASR: laying model
text over aligner words, speaker assignment and sentence grouping, the
hallucination filter, SRT/TXT publication, and long-transcript chunking with
map/reduce prompt assembly. The fixture is a seeded synthetic three-hour
Korean meeting, so every run measures the same workload without network or
ML dependencies.

Prints `METRIC name=value` lines. Exits non-zero if the pipeline output stops
being well formed.
"""

from __future__ import annotations

import hashlib
import random
import statistics
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "worker"))

from galpi_worker.artifacts import (
    Segment,
    filter_segments,
    write_outputs_atomic,
)
from galpi_worker.minutes_pipeline import (
    MAP_SYSTEM_PROMPT,
    MinutesContext,
    build_map_messages,
    build_reduce_messages,
    split_transcript,
)
from galpi_worker.minutes_template import SYSTEM_PROMPT
from galpi_worker.qwen3 import (
    SpeakerTurn,
    TimestampEntry,
    build_word_spans,
    group_word_spans,
)

SEED = 20261009
MEETING_SECONDS = 3 * 60 * 60
SPEAKERS = ("SPEAKER_00", "SPEAKER_01", "SPEAKER_02", "SPEAKER_03")
WORDS = (
    "회의",
    "결정",
    "배포",
    "일정",
    "담당",
    "확인",
    "리스크",
    "고객",
    "서버",
    "화면",
    "데이터",
    "검토",
    "다음",
    "주까지",
    "정리",
    "하겠습니다",
    "그러면",
    "이번",
    "버전",
    "테스트",
    "릴리스",
    "3.14",
    "API",
    "갈피",
    "전사",
    "모델",
    "성능",
    "개선",
    "문서",
)
REPEATS = 5


def fixture() -> tuple[str, list[TimestampEntry], list[SpeakerTurn]]:
    rng = random.Random(SEED)
    pieces: list[str] = []
    entries: list[TimestampEntry] = []
    turns: list[SpeakerTurn] = []
    clock = 0.0
    speaker = SPEAKERS[0]
    turn_start = 0.0
    while clock < MEETING_SECONDS:
        if rng.random() < 0.06:
            turns.append(SpeakerTurn(start=turn_start, end=clock, speaker=speaker))
            speaker = rng.choice(SPEAKERS)
            clock += rng.uniform(0.1, 1.2)
            turn_start = clock
        word = rng.choice(WORDS)
        duration = rng.uniform(0.18, 0.6)
        entries.append(TimestampEntry(text=word, start=clock, end=clock + duration))
        clock += duration + rng.uniform(0.0, 0.25)
        mark = rng.random()
        pieces.append(word + ("." if mark < 0.08 else "," if mark < 0.15 else "") + " ")
    turns.append(SpeakerTurn(start=turn_start, end=clock, speaker=speaker))
    return "".join(pieces), entries, turns


def run_once(
    text: str, entries: list[TimestampEntry], turns: list[SpeakerTurn], out: Path
) -> tuple[int, int, int, str]:
    spans = build_word_spans(text, entries)
    segments: list[Segment] = group_word_spans(spans, turns)
    kept, filtered = filter_segments(segments, MEETING_SECONDS)
    srt = out / "bench.srt"
    txt = out / "bench_화자별.txt"
    write_outputs_atomic(srt, txt, kept)
    transcript = txt.read_text(encoding="utf-8")
    context = MinutesContext(
        background="제품: 갈피",
        participants="- 김갈피 (PM)",
        glossary="- 갈피",
        meeting_date="2026-10-09",
    )
    chunks = split_transcript(transcript)
    notes = [
        build_map_messages(chunk, context)[1]["content"][-400:] for chunk in chunks
    ]
    build_reduce_messages(notes, context, SYSTEM_PROMPT)
    digest = hashlib.sha256(srt.read_bytes()).hexdigest()[:12]
    return len(kept), len(filtered), len(chunks), digest


def main() -> int:
    text, entries, turns = fixture()
    timings: list[float] = []
    result: tuple[int, int, int, str] | None = None
    with tempfile.TemporaryDirectory() as raw:
        out = Path(raw)
        run_once(text, entries, turns, out)  # warm-up
        for _ in range(REPEATS):
            started = time.perf_counter()
            current = run_once(text, entries, turns, out)
            timings.append((time.perf_counter() - started) * 1000)
            if result is not None and current != result:
                print("output differs between runs", file=sys.stderr)
                return 1
            result = current
    assert result is not None
    kept, filtered, chunks, digest = result
    if kept == 0 or chunks == 0:
        print("pipeline produced no segments or chunks", file=sys.stderr)
        return 1
    print(f"ASI words={len(entries)} turns={len(turns)} kept={kept} srt_sha={digest}")
    print(f"METRIC postprocess_ms={statistics.median(timings):.1f}")
    print(f"METRIC kept_segments={kept}")
    print(f"METRIC filtered_segments={filtered}")
    print(f"METRIC map_chunks={chunks}")
    print(f"METRIC system_prompt_chars={len(SYSTEM_PROMPT)}")
    print(f"METRIC map_prompt_chars={len(MAP_SYSTEM_PROMPT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
