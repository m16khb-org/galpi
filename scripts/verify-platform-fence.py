#!/usr/bin/env python3
"""Fail if platform cfg leaks into domain/application/inbound Rust code."""

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
RUST = ROOT / "src-tauri" / "src"
DIRS = [RUST / "domain", RUST / "application", RUST / "adapters" / "inbound"]
PATTERN = re.compile(r"cfg[(](windows|unix)|target_os|cfg![(]")


def main() -> int:
    reasons: list[str] = []
    result = subprocess.run(["bun", "run", "architecture:check"], cwd=ROOT, capture_output=True, text=True)
    if result.returncode != 0:
        reasons.append("architecture:check failed:\n" + result.stdout + result.stderr)
    for directory in DIRS:
        for path in sorted(directory.rglob("*.rs")):
            for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if PATTERN.search(line):
                    reasons.append(f"{path.relative_to(ROOT)}:{number}: {line.strip()}")
    if reasons:
        print("\n".join(reasons))
        return 1
    print("PLATFORM_FENCE_OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
