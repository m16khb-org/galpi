# Installing Galpi with a coding agent

Galpi does not ship prebuilt binaries. Each person builds it from source on the
computer that will run it, usually by asking a coding agent (Claude Code, Codex,
omp, …) to follow this file. A build made on the same machine has no
downloaded-file marker (macOS quarantine attribute, Windows Mark of the Web), so
it opens without the Gatekeeper warning an unsigned downloaded build triggers
(observed on macOS 27), and Windows normally shows no SmartScreen prompt either.

This file is written for the agent. Report to the user in the user's language.

## Ground rules

- Build and install only. Do not edit source files, commit, push, or open pull
  requests. If the build fails for a reason this file does not cover, stop and
  report the exact error.
- Ask the user before installing system-wide software (Xcode Command Line
  Tools, Visual Studio Build Tools, Rust, Bun). Check first; install only what
  is missing.
- Never ask for, type, or log the user's Google password, Hugging Face token,
  or AI API keys. The user enters those in the app.
- Do not set `GALPI_AUTH_GATEWAY_URL`. Release builds ignore it; it exists for
  local auth-gateway development only.

## 1. Check the platform

| Supported | Not supported |
|---|---|
| macOS 14 or later on Apple Silicon (arm64) | Intel Macs |
| Windows 10/11 x64 | Linux, Windows on ARM |

```bash
# macOS
uname -m                 # must print arm64
sw_vers -productVersion  # must be 14 or later
```

```powershell
# Windows PowerShell
$env:PROCESSOR_ARCHITECTURE   # must print AMD64
[System.Environment]::OSVersion.Version   # Major 10 (Windows 10 and 11)
```

On an unsupported platform, stop and tell the user Galpi cannot run there.

## 2. Prerequisites

| Tool | Required version | Check |
|---|---|---|
| Git | any | `git --version` |
| Rust (rustup) | 1.88 or later, stable | `rustc --version` |
| Bun | 1.3 or later | `bun --version` |
| Tauri CLI | exactly 2.11.4 | `cargo tauri --version` |

The repository's `rust-toolchain.toml` selects the stable channel and adds the
needed components and targets; rustup applies it automatically inside the
checkout.

### macOS

```bash
xcode-select -p || xcode-select --install    # Command Line Tools (clang, codesign, hdiutil)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
curl -fsSL https://bun.sh/install | bash
cargo install tauri-cli --version 2.11.4 --locked
```

Open a new shell (or `source "$HOME/.cargo/env"`) after installing rustup so
`cargo` is on `PATH`.

### Windows

```powershell
winget install --id Git.Git -e
winget install --id Rustlang.Rustup -e
winget install --id Oven-sh.Bun -e
winget install --id Microsoft.VCRedist.2015+.x64 -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Open a new PowerShell window so the new tools are on `PATH`, then:

```powershell
rustup default stable-x86_64-pc-windows-msvc
cargo install tauri-cli --version 2.11.4 --locked
```

- The Visual C++ 2015–2022 x64 redistributable is a runtime requirement; the
  installer does not bundle it.
- The Build Tools "Desktop development with C++" workload (MSVC and the
  Windows SDK) provides `link.exe` for the Rust build.
- The installer bootstraps WebView2 if it is missing (Tauri's default install
  mode). Windows 11 already includes it.

## 3. Get the source

```bash
git clone https://github.com/m16khb-org/galpi.git
cd galpi
```

Build from `main`. For an existing checkout, update it with
`git switch main && git pull --ff-only` and stop if the working tree has local
changes the user did not mention.

## 4. Build

The first build downloads Rust crates and a SHA-256-pinned `uv` binary, and
compiles the release profile. Expect several minutes; it needs network access.

```bash
bun install --frozen-lockfile
```

### macOS

```bash
cargo tauri build --bundles app --ci
```

Result: `src-tauri/target/release/bundle/macos/Galpi.app` (ad-hoc signed). A DMG
is not needed for a local install; `bun run build` additionally makes one.

### Windows

```powershell
bun run build
```

Result: an NSIS installer at `src-tauri\target\release\bundle\nsis\Galpi_<version>_x64-setup.exe`.

## 5. Install

### macOS

Quit Galpi if it is running, then replace the app:

```bash
osascript -e 'quit app id "com.m16khb.galpi"' 2>/dev/null || true
rm -rf /Applications/Galpi.app
ditto src-tauri/target/release/bundle/macos/Galpi.app /Applications/Galpi.app
open /Applications/Galpi.app
```

`spctl --assess` reports `rejected` for this ad-hoc signed build. That is
expected and does not stop it from opening: Gatekeeper only blocks files that
carry the download quarantine attribute, and a local build has none. Do not
change Gatekeeper settings or re-sign the app.

### Windows

Run the installer from step 4. It installs for the current user, so no
administrator rights are needed. `/S` runs it silently:

```powershell
& (Get-ChildItem src-tauri\target\release\bundle\nsis\*.exe | Select-Object -First 1).FullName /S
```

Then start Galpi from the Start menu.

Settings, the app-managed Python engine, and downloaded models live in the app
data folder (`~/Library/Application Support/com.m16khb.galpi` on macOS,
`%LOCALAPPDATA%\com.m16khb.galpi` on Windows). Reinstalling keeps them.

## 6. Hand over to the user

Confirm the app opens on its login screen, then tell the user to finish these
steps themselves (details in the README sections of the same names):

1. **Sign in with Google**: press `Google로 로그인`; the system browser opens.
2. **Hugging Face token** (first engine setup only): accept the
   `pyannote/speaker-diarization-community-1` terms and save a read token in
   `설정`.
3. **Prepare the local engine**: `설정` → `로컬 엔진 준비`. This downloads an
   app-managed Python environment and several GB of models.

## Updating

```bash
git switch main && git pull --ff-only
```

Then repeat steps 4 and 5. If the engine shows `대기` after an update, the
dependency lock changed; press `로컬 엔진 준비` once (models are not downloaded
again).

## Troubleshooting

| Symptom | Fix |
|---|---|
| `no such command: tauri` | `cargo install tauri-cli --version 2.11.4 --locked` |
| macOS: `can't find crate for serde_derive` (or `phf_macros`, `tauri_macros`), `mis-aligned LINKEDIT string pool` | Seen with rustc 1.97 on macOS 27. Run `rustup update stable`, delete `src-tauri/target`, and build again. rustc 1.99 on macOS 27.0.1 builds successfully. |
| Windows: `link.exe` not found or `LNK1181` | The Build Tools C++ workload is missing; install it as in step 2 and reopen the shell. |
| Windows engine setup: `Failed to create Python minor version link directory` | Fixed on `main` (Windows blocks uv's junction); update the source and rebuild. |
| Engine setup fails partway | Press `로컬 엔진 준비` again; it resumes and cleans up partial installs. Check the network. |
| Model download fails with 401/403 | The user must accept the pyannote model terms and save a valid read token. |
