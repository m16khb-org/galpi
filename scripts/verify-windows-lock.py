#!/usr/bin/env python3
"""Verify a committed Windows lock reproduces from worker/requirements.txt."""

from __future__ import annotations

import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKER = ROOT / "worker"
LOCKS = {
    "cpu": "requirements-windows-cpu.lock",
    "cuda": "requirements-windows-cuda.lock",
}
PREFIX = "#    uv pip compile "


def fail(reason: str) -> int:
    print(f"LOCK_DRIFT {reason}")
    return 1


def main(argv: list[str]) -> int:
    if len(argv) != 2 or argv[1] not in LOCKS:
        print("usage: verify-windows-lock.py <cpu|cuda>", file=sys.stderr)
        return 2
    kind = argv[1]
    lock = WORKER / LOCKS[kind]
    committed = lock.read_bytes()
    header = [
        line
        for line in committed.decode("utf-8").splitlines()[:5]
        if line.startswith(PREFIX)
    ]
    if not header:
        return fail(f"{lock.name} has no uv command header")
    args = shlex.split(header[0][len(PREFIX) :])
    if "-o" not in args or args[args.index("-o") + 1] != lock.name:
        return fail(f"header output path is not {lock.name}")

    temp = Path(tempfile.mkdtemp(prefix=".lock-verify-", dir=ROOT))
    try:
        shutil.copyfile(WORKER / "requirements.txt", temp / "requirements.txt")
        result = subprocess.run(
            ["uv", "pip", "compile", *args],
            cwd=temp,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            return fail(f"uv pip compile failed: {result.stderr.strip()[-500:]}")
        if (temp / lock.name).read_bytes() != committed:
            return fail(f"{lock.name} differs from a fresh compile")
    finally:
        shutil.rmtree(temp, ignore_errors=True)
    print(f"LOCK_REPRODUCIBLE {kind}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
