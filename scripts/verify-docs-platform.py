"""Gate G13: supported-platform statements name Windows and drop stale macOS-only claims.

Prints PLATFORM_DOCS_OK and exits 0 when every required phrase is present and no
stale phrase remains; otherwise prints PLATFORM_DOCS_FAIL with reasons and exits 1.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Each document must state the new platform support in these words.
REQUIRED: dict[str, str] = {
    "README.md": "Windows 10/11 x64",
    "README.en.md": "Windows 10/11 x64",
    ".issueops/TECH_STACK.md": "Windows 10/11 x64",
    ".issueops/OPERATIONS.md": "Windows",
    "DESIGN.md": "Windows x64",
}

# Platform statements that were true only while the app shipped for Apple Silicon.
# Patterns are matched against the files that carried them, so legitimate uses of
# "Apple Silicon" elsewhere (Qwen3/MLX notes) do not trip the gate.
STALE: tuple[tuple[str, str], ...] = (
    (r"Apple Silicon only", "platform target sentence"),
    (r"Intel Mac, Windows, Linux는 지원하지 않습니다", "Korean README support quote"),
    (r"Intel Macs, Windows, and Linux are not supported", "English README support quote"),
    (r"Add signed Intel/Windows packages", "DESIGN.md accepted-debt row"),
    (r"macOS 14[+] on Apple Silicon;", "single-platform prerequisite"),
    (r"hardcode ARM64", "build-script architecture note"),
)

STALE_SCOPE = (
    "README.md",
    "README.en.md",
    ".issueops/TECH_STACK.md",
    ".issueops/OPERATIONS.md",
    ".issueops/operations/guides/overview.md",
    "DESIGN.md",
    "AGENTS.md",
)


def main() -> int:
    failures: list[str] = []
    for relative, phrase in REQUIRED.items():
        text = (ROOT / relative).read_text(encoding="utf-8")
        if phrase not in text:
            failures.append(f"{relative}: missing '{phrase}'")
    for relative in STALE_SCOPE:
        text = (ROOT / relative).read_text(encoding="utf-8")
        for pattern, label in STALE:
            for match in re.finditer(pattern, text):
                line = text.count("\n", 0, match.start()) + 1
                failures.append(f"{relative}:{line}: stale {label}")
    if failures:
        print("PLATFORM_DOCS_FAIL")
        for failure in failures:
            print(failure)
        return 1
    print("PLATFORM_DOCS_OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
