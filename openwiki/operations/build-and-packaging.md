---
type: operations
title: Build, Staging & Packaging
description: The Bun/Tauri build pipeline for Galpi — checksum-verified staging of the uv sidecar and Python worker, Tauri bundle configuration, custom hdiutil DMG assembly, the tag-triggered release workflow with optional signing and notarization, and the build-time requirements fingerprint that invalidates installed engines.
tags: [build, packaging, release, tauri, dmg, staging, sidecars, uv, codesign, notarization, ci, arm64, fingerprint, vite]
sources:
  - id: openwiki-source-164e2da859b5277df81c7d94
    resource: repo://.github/workflows/ci.yml
  - id: openwiki-source-4d1d392666be6dfdd7a91a2e
    resource: repo://.github/workflows/release.yml
  - id: openwiki-source-ea70eb6c045047448e446296
    resource: repo://.gitignore
  - id: openwiki-source-5b54a58d1b51cd490b0e7162
    resource: repo://package.json
  - id: openwiki-source-b7793decf9d7c9ba48e57e0f
    resource: repo://rust-toolchain.toml
  - id: openwiki-source-b6c5149c993339659b6f4449
    resource: repo://scripts/build-dmg.ts
  - id: openwiki-source-6229fc7315005e295371fb06
    resource: repo://scripts/stage-sidecars.ts
  - id: openwiki-source-7d9c2c95cd6f848332130fbd
    resource: repo://src-tauri/build.rs
  - id: openwiki-source-00ff4b2512b6dbfa268cbfa4
    resource: repo://src-tauri/capabilities/default.json
  - id: openwiki-source-ca67060e890937010b96de80
    resource: repo://src-tauri/Cargo.toml
  - id: openwiki-source-4c1f575b7fe2cf41eccc776e
    resource: repo://src-tauri/Entitlements.plist
  - id: openwiki-source-3603704595d6f088d32021e8
    resource: repo://src-tauri/Info.plist
  - id: openwiki-source-87d1f8af141955ca8bda47d2
    resource: repo://src-tauri/src/adapters/outbound/environment.rs
  - id: openwiki-source-e1ec47b0582abc13e4d0936f
    resource: repo://src-tauri/src/adapters/outbound/paths.rs
  - id: openwiki-source-ba89bf8df1114fffa3babd25
    resource: repo://src-tauri/src/adapters/outbound/setup.rs
  - id: openwiki-source-412847537bfb5d6c5c01305a
    resource: repo://src-tauri/src/composition.rs
  - id: openwiki-source-0abfee918aaf0d7e3ea712fc
    resource: repo://src-tauri/tauri.conf.json
  - id: openwiki-source-5e1b077422a94ae165e88e4e
    resource: repo://vite.config.ts
generated: { by: "openwiki/0.4.3", at: "2026-09-29T14:35:32.287Z" }
verified:
  - by: openwiki/0.4.3
    at: 2026-09-29T14:35:32.287Z
---

# Build, Staging & Packaging

Galpi ships as a single `.app` (and DMG) for macOS 14+ on Apple Silicon. Building
it means assembling four runtimes into one bundle: the Vite-built webview, the
Rust/Tauri host, a checksum-verified `uv` binary, and a staged copy of the
Python worker with its lock files. This page covers the command map, the sidecar
staging script, the Tauri bundle configuration, the compile-time requirements
fingerprint, DMG assembly, and the release workflow with its signing state.
The engine environments these artifacts feed at runtime are described in
[engine presets & environment readiness](../concepts/engines-and-environment.md);
the worker itself in [python worker](../architecture/python-worker.md); the local
and CI verification gates in [verification gates](../testing/verification-gates.md);
and the first-run setup that consumes the staged resources in
[engine setup](../workflows/engine-setup.md).

## The command map

`package.json` is the canonical entry point for every workflow; Bun (>= 1.3.0,
pinned via `packageManager: bun@1.3.14`) is the only supported driver:

| Command | What it runs |
|---|---|
| `bun run dev` | `cargo tauri dev` — whose `beforeDevCommand` stages sidecars, then starts `vite --port 1420 --strictPort`; the window loads `devUrl` `http://localhost:1420` |
| `bun run build` | `cargo tauri build --bundles app --ci` followed by `scripts/build-dmg.ts` (`dmg:build`) via `hdiutil` |
| `bun run sidecar:stage` | `scripts/stage-sidecars.ts` — refresh `src-tauri/binaries/` and `src-tauri/resources/worker/` |
| `bun run vite:dev` | Vite dev server on port 1420, strict port |
| `bun run vite:build` | `tsc --noEmit && vite build` — type check then emit `dist/` |
| `bun run check` | `architecture:check` (the `scripts/check-architecture.ts` fence), Biome lint, `tsc --noEmit` |
| `bun run check:rust` | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` |
| `bun run check:worker` | `uvx ruff check worker`, `uvx ruff format --check worker`, `PYTHONPATH=. python3 -m unittest discover -s worker/tests -t .` |
| `bun run check:all` | `check` + `bun test` + `check:rust` + `check:worker` |

Both Tauri before-commands stage sidecars **first** — dev and release builds
alike never start without the pinned `uv` binary and the worker resources in
place, because the host resolves them at first use (see
[resolution at runtime](#resolution-at-runtime-debug-vs-release)).

```mermaid
flowchart TD
    subgraph devflow["bun run dev"]
        D1["bun run sidecar:stage"] --> D2["bun run vite:dev - port 1420 strictPort"]
        D2 --> D3["cargo tauri dev - window loads devUrl localhost:1420"]
    end
    subgraph relflow["bun run build"]
        B1["bun run sidecar:stage"] --> B2["bun run vite:build - tsc noEmit then vite build"]
        B2 --> B3["cargo tauri build - bundles app -ci"]
        B3 --> B4["scripts/build-dmg.ts via hdiutil"]
        B4 --> B5["Galpi version aarch64 dmg in bundle/dmg"]
    end
```

*The two entry pipelines. Both begin with sidecar staging; only the build path
emits `dist/`, which `tauri.conf.json`'s `frontendDist` points at for the
bundled app.*

## Sidecar staging (`scripts/stage-sidecars.ts`)

The staging script has two independent jobs, run on every dev/build start.

### Pinned `uv` with SHA-256 verification

`stageUv()` guarantees `src-tauri/binaries/uv-aarch64-apple-darwin` matches
`uv` 0.12.5 for `aarch64-apple-darwin`:

- Two checksums are pinned in source: the release tarball's SHA-256 and the
  extracted binary's SHA-256.
- If the binary already exists and hashes to the pinned binary checksum,
  staging short-circuits — no network round trip.
- Otherwise the script downloads
  `https://github.com/astral-sh/uv/releases/download/0.12.5/uv-aarch64-apple-darwin.tar.gz`,
  verifies the **archive** checksum (mismatch raises `SidecarStageError` and the
  build stops), extracts it, writes the binary into `src-tauri/binaries/`, and
  `chmod`s it to `0755`.

This is the binary `tauri.conf.json` declares as `externalBin: ["binaries/uv"]`;
Tauri embeds it next to the executable in release builds.

### The staged worker copy

`stageWorker()` **wipes** `src-tauri/resources/worker/` and rebuilds it from
`worker/`:

- `worker/galpi_worker/` copied recursively, filtering out `__pycache__`
  directories and `.pyc` files.
- The four pin files travel together:
  `requirements.txt`, `requirements.lock`, `requirements-qwen3.txt`,
  `requirements-qwen3.lock`. The loose `.txt` files document the direct pins;
  the `.lock` files are what the installer actually reads (`uv pip install -r …
  --require-hashes` for WhisperX).

The staging targets are **generated output, not source**: `src-tauri/binaries/`
and `src-tauri/resources/worker/` are both gitignored. Never edit a staged copy
— worker changes go to `worker/` and are re-staged by the next dev/build run.
Editing a staged copy would be silently overwritten and never reaches a release.

## Tauri configuration (`src-tauri/tauri.conf.json`)

| Section | Decision |
|---|---|
| Identity | `productName: "Galpi"`, `version: "0.1.0"`, `identifier: "com.m16khb.galpi"` |
| Build | `beforeDevCommand` / `beforeBuildCommand` both start with `bun run sidecar:stage`; dev serves on port 1420 (`devUrl`), build runs `bun run vite:build`; `frontendDist: "../dist"` is what the bundled app loads |
| Window | Single window titled 갈피, 1240×820 (min 920×640), centered, resizable |
| CSP | Locked down: `default-src 'self'`, `img-src 'self' asset: data:`, `style-src 'self' 'unsafe-inline'`, `connect-src ipc: http://ipc.localhost`, `object-src 'none'`, `frame-src 'none'`, `form-action 'none'` |
| Bundle | `targets: ["app"]` only — Tauri's own DMG step never runs; `externalBin: ["binaries/uv"]`; `resources: ["resources/worker"]`; five icon files |
| macOS | `minimumSystemVersion: "14.0"`, `entitlements: "Entitlements.plist"`, `signingIdentity: "-"` (ad-hoc) |

Three of these decisions have direct operational consequences:

1. **`frontendDist` is a build-order constraint.** The `tauri::generate_context!`
   macro in `composition.rs` expands against the `dist/` output, so any Rust
   build or `cargo test` requires `bun run vite:build` to have run first. The CI
   Rust job encodes this ordering explicitly.
2. **`targets: ["app"]` routes DMG work to the custom script.** Tauri produces
   only `Galpi.app`; `scripts/build-dmg.ts` turns it into a DMG afterward.
3. **`signingIdentity: "-"` is the always-valid fallback.** Ad-hoc signing
   requires no certificate, so every checkout can produce a runnable `.app`;
   a real identity arrives only through the `APPLE_SIGNING_IDENTITY` env var in
   CI (see [release workflow](#release-workflow-releaseyml)).

## The requirements fingerprint (`build.rs`)

`src-tauri/build.rs` runs before every compilation and fingerprints the worker's
pin files:

- It hashes `worker/requirements.txt` and `worker/requirements-qwen3.txt` with
  FNV-1a (a deliberate choice: the question is "did this file change", not "can
  an attacker forge this") and emits the 16-hex-digit digests as compile-time
  environment variables `GALPI_WHISPERX_REQUIREMENTS_HASH` and
  `GALPI_QWEN3_REQUIREMENTS_HASH`.
- It emits `cargo::rerun-if-changed` for both files, so editing a pin forces a
  rebuild of the crate that embeds the hash.
- A missing requirements file is a broken checkout: the build stops with
  `cargo::error` rather than compiling in a fingerprint of nothing.
- Only then does it call `tauri_build::build()` for the normal Tauri codegen.

`environment.rs` consumes the variables at compile time and composes readiness
markers: `whisperx_marker()` is `3.8.6` plus the WhisperX hash, `qwen3_marker()`
is `2` plus the Qwen3 hash. `setup.rs` writes the matching marker file after a
successful venv install (`engine/ready-3.8.6` or
`engine/qwen3/ready-qwen3-<version>`), and `status()` counts an engine ready
only when the on-disk marker string equals the compiled one.

The consequence is the mechanism's whole point: **editing a pinned dependency
invalidates every installed virtualenv on its own.** The next build compiles a
new hash, the marker comparison fails, `diagnose` reports the engine not ready,
and the next `prepare` reinstalls the venv and rewrites the marker — no version
constant bump and no human memory required. The readiness side of this loop is
detailed in [engine presets & environment readiness](../concepts/engines-and-environment.md).

## DMG assembly (`scripts/build-dmg.ts`)

The DMG script runs only after `cargo tauri build --bundles app --ci` has
produced `src-tauri/target/release/bundle/macos/Galpi.app` (verified by the
presence of `Contents/Info.plist`; absence raises `DmgBuildError`).

- **The artifact name follows the version in `tauri.conf.json`** — the same
  file the bundler reads — producing `Galpi_<version>_aarch64.dmg`. A missing
  or empty `version` fails the build, so a release can never ship an artifact
  named after the previous version.
- It first cleans stale output: the whole `bundle/dmg` directory, the
  `bundle/share` directory, and any `rw.*.dmg` mount droppings left in
  `bundle/macos` by interrupted `hdiutil` runs.
- It stages a fresh temp directory containing a copy of `Galpi.app` and an
  `/Applications` symlink (the drag-to-install affordance), then runs
  `hdiutil create -volname Galpi -srcfolder <staging> -ov -format ULMO <dmg>`.
  `ULMO` is the LZMA-compressed UDIF format, the smallest practical
  distribution image.
- Subprocess failures (any non-zero exit) raise `DmgBuildError`; the staging
  temp directory is removed in a `finally` block either way.

Bumping the release version therefore means editing `version` in
`src-tauri/tauri.conf.json` (and keeping the `v`-tag in step); the DMG filename,
the bundle's `CFBundleShortVersionString`, and the release artifact all follow.

## Release workflow (`release.yml`)

The Release workflow triggers on pushing a `v*` tag or on manual
`workflow_dispatch`. Its single `dmg` job runs on `macos-15` (Apple Silicon —
the only platform the app targets) with `contents: write`.

```mermaid
flowchart TD
    TRG["push tag v* or workflow_dispatch"] --> JOB["dmg job on macos-15"]
    JOB --> CERT{"APPLE_CERTIFICATE secret present?"}
    CERT -->|yes| IMPORT["create build keychain and import p12 for codesign"]
    CERT -->|no| SKIP["skip import"]
    IMPORT --> BLDDMG["bun run build - APPLE_SIGNING_IDENTITY may be empty"]
    SKIP --> BLDDMG
    BLDDMG --> ID{"APPLE_SIGNING_IDENTITY present?"}
    ID -->|yes| VERIFY["codesign verify strict plus spctl assess"]
    ID -->|no| ADHOC["ad-hoc signed DMG for internal testing"]
    VERIFY --> UP["upload galpi-dmg artifact"]
    ADHOC --> UP
```

*The signing branch of the release job. Both branches produce a DMG; only the
identity-present branch is expected to pass Gatekeeper for external recipients.*

Steps, in order:

1. **Setup.** Checkout, Bun (version taken from `package.json` via
   `bun-version-file`), Rust cache scoped to `src-tauri`, and
   `bun install --frozen-lockfile`.
2. **Optional certificate import.** Runs only when the `APPLE_CERTIFICATE`
   secret is non-empty: it creates a dedicated `build.keychain`, decodes and
   imports the base64 p12, and sets the key partition list so `codesign` can
   use it non-interactively. Without the secret the step is skipped.
3. **Build the DMG** (`bun run build`). The signing/notarization secrets are
   exported as env vars — `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
   `APPLE_PASSWORD`, `APPLE_TEAM_ID` — and are **empty when absent**, which
   leaves Tauri on its configured ad-hoc identity. With secrets present, Tauri
   signs with the identity and submits for notarization (stapling the ticket);
   the entitlements from `Entitlements.plist` are applied to the hardened
   runtime.
4. **Gatekeeper verification.** Runs only when `APPLE_SIGNING_IDENTITY` is
   non-empty: `codesign --verify --deep --strict --verbose=2` on
   `src-tauri/target/release/bundle/macos/Galpi.app`, then
   `spctl --assess --type execute` — the check a recipient's Mac actually
   performs on a notarized, stapled build. A failure fails the release.
5. **Upload artifact.** `galpi-dmg` collects
   `src-tauri/target/release/bundle/dmg/*.dmg` with `if-no-files-found: error`,
   so a build that silently failed to produce a DMG cannot pass unnoticed.

Two operational notes: the release job itself runs **no test gates** — quality
checks live in the CI workflow, so a release tag is expected to point at a
commit CI has already verified. And because the signing secrets are optional by
design, the common state today is the ad-hoc branch: the DMG installs and runs
for internal testing, but recipients outside the build machine will need to
bypass Gatekeeper manually. `Entitlements.plist`'s Hardened Runtime exemptions
are documented in-source as not yet exercised by any signed build.

## Resolution at runtime: debug vs release

The host resolves both sidecars differently by build profile
(`paths.rs`), which is why staging must precede every build:

| Symbol | Debug build | Release build |
|---|---|---|
| `uv_binary()` | `src-tauri/binaries/uv-aarch64-apple-darwin` (via `CARGO_MANIFEST_DIR`) | `uv` beside the current executable — where Tauri places the `externalBin` |
| `worker_root()` | the repo checkout `../worker` | `<resource_dir>/resources/worker` — the staged copy |

Debug builds therefore always exercise the live worker source, and release
builds exercise exactly what `stage-sidecars.ts` staged.

## Toolchain and platform constraints

- **ARM64-only, deliberately.** Every build script hardcodes
  `aarch64-apple-darwin`: the staged `uv`, the DMG suffix (`_aarch64.dmg`), and
  the Rust target. `rust-toolchain.toml` tracks `stable` (with `clippy` and
  `rustfmt` components) and guarantees only the target every gate needs; the
  enforced MSRV is `rust-version = "1.88"` in `src-tauri/Cargo.toml` (needed
  for let-chains).
- **All CI and release jobs run on `macos-15`.** The app ships only for arm64
  macOS 14+, so the gates run on the platform they describe; Vite builds with
  `target: "safari17"` because the window ships inside the WKWebView that
  macOS 14+ provides — there is no older engine to down-level for.
- **Release profile.** `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`,
  `strip = true`. `panic` stays at the default unwind on purpose: aborting
  would turn a panic in the audio writer thread into a lost recording instead
  of a failed one.
- **Generated trees are not source.** `.gitignore` excludes every output the
  pipeline regenerates — `node_modules/`, `dist/`, `src-tauri/target/`,
  `src-tauri/gen/`, `src-tauri/binaries/`, `src-tauri/resources/worker/`, and
  the Python caches (`**/__pycache__/`, `*.pyc`). All of these are
  never-source, never-edit outputs; only `worker/`, `tauri.conf.json`,
  `build.rs`, the workflow files, and the scripts are edited by hand.

## Bundle metadata

- `src-tauri/Entitlements.plist` grants microphone access
  (`com.apple.security.device.audio-input`) plus three Hardened Runtime
  exemptions required once a build is notarized:
  `disable-library-validation` (the app loads PyTorch/MLX libraries from its
  self-installed Python environment, not signed by us), `allow-jit` and
  `allow-unsigned-executable-memory` (the Metal backend compiles shaders into
  executable memory). Without them the engine cannot load in a notarized build.
- `src-tauri/Info.plist` carries the microphone usage description string macOS
  shows at first recording.
- `src-tauri/capabilities/default.json` scopes the window's Tauri permissions:
  event listen/unlisten, the dialog open permission, and the opener plugin
  restricted to `https://huggingface.co/*` URLs.

## Failure semantics summary

| Failure | Where | Effect |
|---|---|---|
| `uv` archive checksum mismatch | `stage-sidecars.ts` | `SidecarStageError`, build stops before compilation |
| Missing requirements file | `build.rs` | `cargo::error`, compile fails |
| Missing `Galpi.app` or version | `build-dmg.ts` | `DmgBuildError`, DMG not built |
| Non-zero `hdiutil` exit | `build-dmg.ts` | `DmgBuildError`, staging temp cleaned in `finally` |
| Gatekeeper check fails (identity present) | `release.yml` | Release job fails before upload |
| No DMG found at upload | `release.yml` | `if-no-files-found: error` fails the job |
