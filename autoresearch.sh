#!/usr/bin/env bash
# Canonical benchmark: worker correctness gate, then speaker attribution of
# transcript lines replayed on cached real-meeting diarization (AI Hub
# dataSetSn=132, local only; see scripts/bench/diarization_eval.py). The
# transcript-correction benchmark and minutes-prompt rubric are guards.
set -euo pipefail
cd "$(dirname "$0")"

PYTHONPATH=. python3 -m unittest discover -s worker/tests -t . >/tmp/autoresearch-tests.log 2>&1 || {
  tail -30 /tmp/autoresearch-tests.log >&2
  echo "worker unit tests failed" >&2
  exit 1
}
uvx --offline ruff check worker scripts/bench >&2

python3 scripts/bench/diarization_eval.py
python3 scripts/bench/correction_eval.py | grep -E '^METRIC correction_score='
python3 scripts/bench/minutes_prompt_eval.py | grep -E '^METRIC prompt_score='
