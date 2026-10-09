#!/usr/bin/env bash
# Canonical benchmark: worker correctness gate, then the deterministic
# post-processing benchmark. No network, fixed seed.
set -euo pipefail
cd "$(dirname "$0")"

PYTHONPATH=. python3 -m unittest discover -s worker/tests -t . >/tmp/autoresearch-tests.log 2>&1 || {
  tail -30 /tmp/autoresearch-tests.log >&2
  echo "worker unit tests failed" >&2
  exit 1
}
uvx --offline ruff check worker scripts/bench >&2

python3 scripts/bench/worker_bench.py
