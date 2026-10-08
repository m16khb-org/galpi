use std::path::Path;

/// Requirements files whose contents decide whether an installed engine is
/// still current. Their fingerprints are compiled in so that editing a pinned
/// dependency invalidates every existing virtualenv on its own, instead of
/// waiting for someone to remember to bump a version constant by hand.
const REQUIREMENTS: [(&str, &str); 4] = [
    ("GALPI_WHISPERX_REQUIREMENTS", "../worker/requirements.txt"),
    (
        "GALPI_QWEN3_REQUIREMENTS",
        "../worker/requirements-qwen3.txt",
    ),
    (
        "GALPI_WHISPERX_WIN_CPU_LOCK",
        "../worker/requirements-windows-cpu.lock",
    ),
    (
        "GALPI_WHISPERX_WIN_CUDA_LOCK",
        "../worker/requirements-windows-cuda.lock",
    ),
];

fn main() {
    for (variable, path) in REQUIREMENTS {
        println!("cargo::rerun-if-changed={path}");
        let Ok(contents) = std::fs::read(Path::new(path)) else {
            // A missing requirements file is a broken checkout, and the build
            // must stop rather than compile in a fingerprint of nothing.
            println!("cargo::error=failed to read {path}");
            return;
        };
        println!(
            "cargo::rustc-env={variable}_HASH={:016x}",
            fingerprint(&contents)
        );
    }
    tauri_build::build();
}

/// FNV-1a over the file bytes.
///
/// This answers "did this file change", not "can an attacker forge this", so a
/// short stable hash beats pulling a cryptographic digest into the build.
fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
