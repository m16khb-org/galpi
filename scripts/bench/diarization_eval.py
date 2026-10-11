#!/usr/bin/env python3
"""Speaker attribution of Galpi's transcript lines against gold speakers.

Replays the post-diarization step on cached engine output: for each meeting
the aligned WhisperX checkpoint, the pyannote community-1 turns and speaker
centroids, the voice of every kept line (`galpi_worker.voice.line_voices`),
and gold utterance spans with speakers. Nothing here runs a model, so the
score is deterministic.

The data is licensed (AI Hub) and never committed; point
`GALPI_DIARIZATION_DATA` at a directory holding

    meetings/<name>.gold.json        [{start, end, speaker, text}]
    runs/<name>/<name>.aligned.v2.json
    diar-cache/<name>.turns.json     {regular, exclusive, embeddings}
    diar-cache/<name>.voices.json    {spans, voices}

Metric: word diarization error rate (WDER). Every transcript word gets the
speaker of the line it ends up in; system labels map one-to-one onto gold
speakers to maximize agreement; a word counts as wrong when its mapped label
differs from the gold speaker at the word's midpoint (or the nearest gold
utterance in a gap). Unlabelled lines are always wrong.
"""

import json
import os
import sys
from collections import Counter
from itertools import permutations
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "worker"))

from galpi_worker.artifacts import filter_segments
from galpi_worker.speakers import label_segments, relabel_by_voice

DATA = Path(os.environ.get("GALPI_DIARIZATION_DATA", "/private/tmp/aihub-staging"))


def gold_speaker(gold: list[dict], time: float) -> str:
    best, gap = gold[0]["speaker"], float("inf")
    for utterance in gold:
        if utterance["start"] <= time <= utterance["end"]:
            return utterance["speaker"]
        distance = min(abs(time - utterance["start"]), abs(time - utterance["end"]))
        if distance < gap:
            best, gap = utterance["speaker"], distance
    return best


def words_of(segment: dict) -> list[tuple[float, float]]:
    timed = [
        (w["start"], w.get("end", w["start"]))
        for w in segment.get("words", [])
        if "start" in w
    ]
    if timed:
        return timed
    count = max(1, len(segment["text"].split()))
    return [(segment["start"], segment["end"])] * count


def best_mapping(pairs: Counter) -> dict[str, str]:
    labels = sorted({label for label, _ in pairs})
    golds = sorted({gold for _, gold in pairs})
    if len(labels) <= 7 and len(golds) <= 8:
        best: dict[str, str] = {}
        best_score = -1
        size = min(len(labels), len(golds))
        for chosen in permutations(golds, size) if len(labels) <= len(golds) else []:
            mapping = dict(zip(labels, chosen, strict=False))
            score = sum(n for (l, g), n in pairs.items() if mapping.get(l) == g)
            if score > best_score:
                best, best_score = mapping, score
        if best_score >= 0:
            return best
    mapping: dict[str, str] = {}
    used: set[str] = set()
    for (label, gold), _ in pairs.most_common():
        if label not in mapping and gold not in used:
            mapping[label] = gold
            used.add(gold)
    return mapping


def evaluate(name: str) -> tuple[int, int, int, int]:
    gold = json.loads(
        (DATA / "meetings" / f"{name}.gold.json").read_text(encoding="utf-8")
    )
    checkpoint = json.loads(
        (DATA / "runs" / name / f"{name}.aligned.v2.json").read_text(encoding="utf-8")
    )
    cached = json.loads(
        (DATA / "diar-cache" / f"{name}.turns.json").read_text(encoding="utf-8")
    )
    voices = json.loads(
        (DATA / "diar-cache" / f"{name}.voices.json").read_text(encoding="utf-8")
    )["voices"]
    duration = max(g["end"] for g in gold)
    # The engine's order: label from the regular turns, drop hallucinations,
    # then relabel each kept line by its own voice.
    labelled = label_segments(checkpoint["segments"], cached["regular"])
    kept, _ = filter_segments(labelled, duration)
    kept = relabel_by_voice(kept, voices, cached["embeddings"])
    pairs: Counter = Counter()
    words: list[tuple[str, str]] = []
    for segment in kept:
        label = segment.get("speaker", "UNKNOWN")
        for start, end in words_of(segment):
            truth = gold_speaker(gold, (start + end) / 2)
            words.append((label, truth))
            if label != "UNKNOWN":
                pairs[(label, truth)] += 1
    mapping = best_mapping(pairs)
    wrong = sum(1 for label, truth in words if mapping.get(label) != truth)
    lines_wrong = 0
    for segment in kept:
        label = segment.get("speaker", "UNKNOWN")
        truth = Counter(
            gold_speaker(gold, (s + e) / 2) for s, e in words_of(segment)
        ).most_common(1)[0][0]
        lines_wrong += mapping.get(label) != truth
    return wrong, len(words), lines_wrong, len(kept)


def main() -> int:
    names = sorted(
        p.name.removesuffix(".turns.json")
        for p in (DATA / "diar-cache").glob("*.turns.json")
    )
    if not names:
        print(f"no cached meetings under {DATA}", file=sys.stderr)
        return 1
    totals = Counter()
    for name in names:
        wrong, words, lines_wrong, lines = evaluate(name)
        totals.update(wrong=wrong, words=words, lines_wrong=lines_wrong, lines=lines)
        if "-v" in sys.argv:
            print(
                f"{name}: WDER {wrong / words:.2%} ({wrong}/{words}) lines wrong {lines_wrong}/{lines}"
            )
    print(f"METRIC wder={100 * totals['wrong'] / totals['words']:.3f}")
    print(f"METRIC line_error={100 * totals['lines_wrong'] / totals['lines']:.3f}")
    print(f"METRIC transcript_lines={totals['lines']}")
    print(f"METRIC diar_meetings={len(names)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
