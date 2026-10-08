---
name: 2026-10-09-bun-run-build-fails-on-macos-27-deployment-target-14-0-proc
description: Caution record for a solved false case or recurring risk.
---

# bun run build fails on macOS 27: deployment target 14.0 proc-macro dylibs are rejected

- Date: 2026-10-09
- Kind: `caution`
- Source: issueops-docs
- Summary: cargo tauri build sets MACOSX_DEPLOYMENT_TARGET=14.0 from tauri.conf.json minimumSystemVersion, and on macOS 27.0.1 (ld-27037.1, rustc 1.97.0) the resulting proc-macro dylibs fail to load with 'mis-aligned LINKEDIT string pool', so bun run build stops with can't find crate for serde_derive/phf_macros/tauri_macros.
- Context: Seen while verifying issue #5 (frontend-only change; src-tauri untouched). The failure reproduces in an empty crate outside the repo with only serde derive when MACOSX_DEPLOYMENT_TARGET=14.0 is exported, and the same crate builds without it. Plain cargo build --release --manifest-path src-tauri/Cargo.toml (no deployment target) completed. Corrupt dylibs stay in src-tauri/target, so later builds keep failing until the target directory is removed; CARGO_BUILD_JOBS=1 does not help.
- Resolution: Not fixed in issue #5. Treat this as build-environment debt: do not chase it as a regression of the current diff, record the bun run build gate as environment-blocked with this evidence, and remove src-tauri/target after a failed attempt. Changing minimumSystemVersion or the toolchain is a separate decision with its own issue.
