//! Platform-neutral rules for Windows Credential Manager entries.
//!
//! The FFI lives in `credential_manager`; everything that can be decided
//! without calling Win32 is here so it is tested on every host.
//!
//! # Storage layout
//!
//! A credential blob holds at most [`MAX_BLOB_BYTES`], and a ChatGPT token
//! record is several times that, so a secret may span entries:
//!
//! - **Short** (UTF-16LE fits in one blob): the value itself under the secret's
//!   target. This is the only layout earlier builds wrote, and short secrets
//!   still use it, so existing entries keep reading.
//! - **Long**: the value's UTF-16LE bytes are cut into [`MAX_BLOB_BYTES`] pieces
//!   stored under `<target>#<generation>.<index>`, and the secret's own target
//!   holds a manifest `chunks:<pieces>:<generation>:<bytes>:<sha256>`. The
//!   manifest starts with a lone UTF-16 surrogate, which no `&str` can encode
//!   to, so it can never be mistaken for a short value.
//!
//! # What is guaranteed
//!
//! - **One commit point.** A rewrite stores every piece of a *new* generation
//!   first and replaces the manifest last. Pieces of the generation a reader is
//!   following are never overwritten, so an interrupted write leaves the
//!   previous secret whole, never a splice of two.
//! - **Corruption is an error, not a value.** The manifest carries the piece
//!   count, byte length and SHA-256 of the secret, so a missing, short, foreign
//!   or altered piece reads as `CREDENTIAL_READ_FAILED`.
//! - **No leftovers after a delete.** Cleanup is driven by enumerating
//!   `<target>#*`, not by trusting a count, so pieces orphaned by a crash or a
//!   failed cleanup are removed by the next write or delete. Between a crash
//!   and that next call, unreachable fragments of the older generation can
//!   exist; a delete removes them all.

use super::{SERVICE, Secret};
use crate::application::error::AppError;
use sha2::{Digest, Sha256};

/// `CRED_MAX_CREDENTIAL_BLOB_SIZE`: 5 * 512 bytes, i.e. 1,280 UTF-16 units.
pub const MAX_BLOB_BYTES: usize = 2560;

/// A lone high surrogate (U+D800, little endian), then `chunks:`.
const MANIFEST_PREFIX: &[u8] = &[0x00, 0xD8, b'c', b'h', b'u', b'n', b'k', b's', b':'];

/// `com.m16khb.galpi:hugging-face-token` and friends.
pub fn credential_target(secret: Secret) -> String {
    credential_target_in(SERVICE, secret)
}

/// Like [`credential_target`] under another service, for isolated tests.
pub fn credential_target_in(service: &str, secret: Secret) -> String {
    format!("{service}:{}", secret.account())
}

/// Where piece `index` of generation `generation` of a long secret lives.
pub fn piece_target(target: &str, generation: &str, index: usize) -> String {
    format!("{target}#{generation}.{index}")
}

/// The `CredEnumerateW` filter matching every piece of any generation.
pub fn piece_filter(target: &str) -> String {
    format!("{target}#*")
}

/// UTF-16LE bytes of `value`.
pub fn encode_blob(value: &str) -> Vec<u8> {
    value.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

pub fn decode_blob(bytes: &[u8]) -> Result<String, AppError> {
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(unreadable());
    }
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    String::from_utf16(&units).map_err(|_| unreadable())
}

fn unreadable() -> AppError {
    AppError::new(
        "CREDENTIAL_READ_FAILED",
        "저장된 자격 증명 형식이 올바르지 않습니다.",
    )
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// What to write for one secret: the entry under its own target, plus the
/// pieces it points at (empty for a short secret), in piece order.
pub struct Layout {
    pub primary: Vec<u8>,
    pub pieces: Vec<Vec<u8>>,
}

/// Lay `value` out. `generation` must be unique per write (ASCII letters and
/// digits only), so a new generation never shares a piece name with the live one.
pub fn layout(value: &str, generation: &str) -> Layout {
    let bytes = encode_blob(value);
    if bytes.len() <= MAX_BLOB_BYTES {
        return Layout {
            primary: bytes,
            pieces: Vec::new(),
        };
    }
    let pieces: Vec<Vec<u8>> = bytes.chunks(MAX_BLOB_BYTES).map(<[u8]>::to_vec).collect();
    let mut primary = MANIFEST_PREFIX.to_vec();
    primary.extend_from_slice(
        format!(
            "{}:{generation}:{}:{}",
            pieces.len(),
            bytes.len(),
            digest(&bytes)
        )
        .as_bytes(),
    );
    Layout { primary, pieces }
}

struct Manifest<'a> {
    pieces: usize,
    generation: &'a str,
    bytes: usize,
    digest: &'a str,
}

/// The manifest a primary entry holds: `None` for a short value, an error for
/// a manifest that cannot be understood.
fn manifest(primary: &[u8]) -> Result<Option<Manifest<'_>>, AppError> {
    let Some(fields) = primary.strip_prefix(MANIFEST_PREFIX) else {
        return Ok(None);
    };
    let fields = std::str::from_utf8(fields).map_err(|_| unreadable())?;
    let mut parts = fields.split(':');
    let (Some(pieces), Some(generation), Some(bytes), Some(digest), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return Err(unreadable());
    };
    let pieces: usize = pieces.parse().map_err(|_| unreadable())?;
    let bytes: usize = bytes.parse().map_err(|_| unreadable())?;
    let generation_ok =
        !generation.is_empty() && generation.bytes().all(|byte| byte.is_ascii_alphanumeric());
    let digest_ok = digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit());
    // A manifest is only written for more than one piece, and the count follows
    // from the length.
    if !generation_ok || !digest_ok || pieces < 2 || pieces != bytes.div_ceil(MAX_BLOB_BYTES) {
        return Err(unreadable());
    }
    Ok(Some(Manifest {
        pieces,
        generation,
        bytes,
        digest,
    }))
}

/// Piece targets a primary entry names; empty for a short or unreadable one.
pub fn live_pieces(target: &str, primary: &[u8]) -> Vec<String> {
    manifest(primary)
        .ok()
        .flatten()
        .map(|manifest| {
            (0..manifest.pieces)
                .map(|index| piece_target(target, manifest.generation, index))
                .collect()
        })
        .unwrap_or_default()
}

/// Of the piece entries `existing`, those the current primary entry (`None`
/// when there is none) does not name, which is everything that may be deleted.
pub fn orphans(target: &str, primary: Option<&[u8]>, existing: Vec<String>) -> Vec<String> {
    let live = primary.map_or_else(Vec::new, |primary| live_pieces(target, primary));
    existing
        .into_iter()
        .filter(|name| !live.contains(name))
        .collect()
}

/// Rebuild the secret from its primary entry, fetching each piece by target
/// name through `piece`.
///
/// A piece that is absent is corruption, not "no secret": the manifest said it
/// exists. The joined bytes must match the manifest's length and digest.
pub fn assemble(
    target: &str,
    primary: &[u8],
    mut piece: impl FnMut(&str) -> Result<Option<Vec<u8>>, AppError>,
) -> Result<String, AppError> {
    let Some(manifest) = manifest(primary)? else {
        return decode_blob(primary);
    };
    let mut joined = Vec::new();
    for index in 0..manifest.pieces {
        let name = piece_target(target, manifest.generation, index);
        joined.extend_from_slice(&piece(&name)?.ok_or_else(unreadable)?);
    }
    if joined.len() != manifest.bytes || digest(&joined) != manifest.digest {
        return Err(unreadable());
    }
    decode_blob(&joined)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_BLOB_BYTES, assemble, credential_target, decode_blob, encode_blob, layout, live_pieces,
        orphans, piece_filter, piece_target,
    };
    use crate::adapters::outbound::secrets::Secret;
    use crate::application::error::AppError;
    use std::collections::HashMap;

    const TARGET: &str = "com.m16khb.galpi:chatgpt-tokens";

    /// The entries the Win32 store would hold for a laid-out secret.
    fn store(generation: &str, pieces: &[Vec<u8>]) -> HashMap<String, Vec<u8>> {
        pieces
            .iter()
            .enumerate()
            .map(|(index, piece)| (piece_target(TARGET, generation, index), piece.clone()))
            .collect()
    }

    fn read(primary: &[u8], entries: &HashMap<String, Vec<u8>>) -> Result<String, AppError> {
        assemble(TARGET, primary, |name| Ok(entries.get(name).cloned()))
    }

    fn code(result: Result<String, AppError>) -> Option<String> {
        result.err().map(|error| error.code)
    }

    fn long(units: usize) -> String {
        "galpi-test-".to_owned() + &"x".repeat(units)
    }

    #[test]
    fn targets_are_service_qualified_account_names() {
        assert_eq!(
            credential_target(Secret::HuggingFaceToken),
            "com.m16khb.galpi:hugging-face-token"
        );
        assert_eq!(
            credential_target(Secret::AssistantApiKey),
            "com.m16khb.galpi:assistant-api-key"
        );
        assert_eq!(credential_target(Secret::ChatGptTokens), TARGET);
    }

    #[test]
    fn credential_piece_targets_are_generation_scoped_and_enumerable() {
        let target = piece_target(TARGET, "abc123", 3);
        assert_eq!(target, "com.m16khb.galpi:chatgpt-tokens#abc123.3");
        // The enumeration filter is the prefix of every piece of every generation.
        let filter = piece_filter(TARGET);
        assert_eq!(filter, "com.m16khb.galpi:chatgpt-tokens#*");
        assert!(target.starts_with(filter.trim_end_matches('*')));
    }

    #[test]
    fn blobs_round_trip_including_korean_text() -> Result<(), AppError> {
        for value in ["galpi-test-ascii", "갈피-테스트-🙂", ""] {
            assert_eq!(decode_blob(&encode_blob(value))?, value);
        }
        Ok(())
    }

    #[test]
    fn a_blob_is_utf16_little_endian() {
        assert_eq!(encode_blob("a가"), [0x61, 0x00, 0x00, 0xAC]);
    }

    #[test]
    fn malformed_blobs_are_read_failures() {
        // Odd length, then a lone surrogate (0xD800)
        for bytes in [&[0x61_u8][..], &[0x00, 0xD8][..]] {
            assert_eq!(
                decode_blob(bytes).err().map(|error| error.code).as_deref(),
                Some("CREDENTIAL_READ_FAILED")
            );
        }
    }

    #[test]
    fn a_credential_that_fits_one_blob_keeps_the_single_entry_format() {
        // Given: exactly 1,280 code units, the old format's limit
        let value = "a".repeat(1280);

        // When
        let layout = layout(&value, "gen1");

        // Then: no pieces, and the entry is the bare UTF-16LE value
        assert_eq!(layout.pieces.len(), 0);
        assert_eq!(layout.primary, encode_blob(&value));
        assert_eq!(layout.primary.len(), MAX_BLOB_BYTES);
    }

    #[test]
    fn a_credential_beyond_one_blob_is_split_into_blob_sized_pieces() {
        // Given: one code unit over the limit
        let layout = layout(&"a".repeat(1281), "gen1");

        // Then: the manifest names both pieces and fits one blob itself
        let sizes: Vec<usize> = layout.pieces.iter().map(Vec::len).collect();
        assert_eq!(sizes, [MAX_BLOB_BYTES, 2]);
        assert_eq!(
            live_pieces(TARGET, &layout.primary),
            [
                piece_target(TARGET, "gen1", 0),
                piece_target(TARGET, "gen1", 1)
            ]
        );
        assert!(layout.primary.len() <= MAX_BLOB_BYTES);
    }

    #[test]
    fn credentials_of_any_length_round_trip_through_their_layout() -> Result<(), AppError> {
        // Given: sizes around every boundary, and text whose surrogate pairs
        // straddle a piece edge
        let straddling = format!("{}🙂{}", "a".repeat(1279), "b".repeat(2000));
        let values = [
            String::new(),
            "galpi-test-short".to_owned(),
            "가".repeat(1280),
            "가".repeat(1281),
            straddling,
            long(MAX_BLOB_BYTES * 7 + 1),
        ];
        for value in values {
            // When
            let layout = layout(&value, "gen1");
            let entries = store("gen1", &layout.pieces);

            // Then
            assert_eq!(read(&layout.primary, &entries)?, value);
        }
        Ok(())
    }

    #[test]
    fn credential_entries_written_by_the_single_entry_format_still_read() -> Result<(), AppError> {
        // Given: a blob exactly as earlier builds stored it
        let legacy = encode_blob("galpi-test-값-0123");

        // When: read with no pieces available
        let value = read(&legacy, &HashMap::new())?;

        // Then
        assert_eq!(value, "galpi-test-값-0123");
        assert_eq!(live_pieces(TARGET, &legacy).len(), 0);
        Ok(())
    }

    #[test]
    fn a_missing_credential_piece_is_a_read_failure() {
        // Given: a three-piece secret with its middle piece gone
        let layout = layout(&long(MAX_BLOB_BYTES), "gen1");
        let mut entries = store("gen1", &layout.pieces);
        let _removed = entries.remove(&piece_target(TARGET, "gen1", 1));

        // When / Then
        assert_eq!(
            code(read(&layout.primary, &entries)).as_deref(),
            Some("CREDENTIAL_READ_FAILED")
        );
    }

    #[test]
    fn a_corrupt_credential_piece_is_a_read_failure() {
        // Given: pieces altered without changing the secret's length (ASCII
        // JSON stays valid UTF-16, so only the digest can notice) ...
        let layout = layout(&long(MAX_BLOB_BYTES), "gen1");
        let mut entries = store("gen1", &layout.pieces);
        if let Some(piece) = entries.get_mut(&piece_target(TARGET, "gen1", 0)) {
            piece[0] ^= 1;
        }
        assert_eq!(
            code(read(&layout.primary, &entries)).as_deref(),
            Some("CREDENTIAL_READ_FAILED")
        );

        // ... or shortened so the joined text is not even UTF-16
        let mut entries = store("gen1", &layout.pieces);
        if let Some(piece) = entries.get_mut(&piece_target(TARGET, "gen1", 0)) {
            let _last = piece.pop();
        }
        assert_eq!(
            code(read(&layout.primary, &entries)).as_deref(),
            Some("CREDENTIAL_READ_FAILED")
        );
    }

    #[test]
    fn two_credential_generations_never_splice_into_a_value() {
        // Given: an interrupted replacement left pieces of an older generation
        // under the live manifest's names, same length, different content
        let old = layout(&long(MAX_BLOB_BYTES).replace('x', "o"), "gen1");
        let new = layout(&long(MAX_BLOB_BYTES), "gen2");
        let mut entries = store("gen2", &new.pieces);
        let stale = store("gen2", &old.pieces);
        if let (Some(first), Some(old_first)) = (
            entries.get_mut(&piece_target(TARGET, "gen2", 0)),
            stale.get(&piece_target(TARGET, "gen2", 0)),
        ) {
            first.clone_from(old_first);
        }

        // When / Then: the digest rejects the mixed set
        assert_eq!(
            code(read(&new.primary, &entries)).as_deref(),
            Some("CREDENTIAL_READ_FAILED")
        );
    }

    #[test]
    fn a_failing_piece_read_is_passed_through() {
        let layout = layout(&long(MAX_BLOB_BYTES), "gen1");
        let read = assemble(TARGET, &layout.primary, |_| {
            Err(AppError::new("CREDENTIAL_READ_FAILED", "access denied"))
        });
        assert_eq!(code(read).as_deref(), Some("CREDENTIAL_READ_FAILED"));
    }

    #[test]
    fn malformed_credential_manifests_are_read_failures() {
        let good = layout(&long(MAX_BLOB_BYTES), "gen1").primary;
        let text = String::from_utf8_lossy(&good[super::MANIFEST_PREFIX.len()..]).into_owned();
        let digest = text.rsplit(':').next().unwrap_or_default().to_owned();
        let bad: Vec<String> = vec![
            String::new(),
            "2".to_owned(),
            "2:gen1".to_owned(),
            "3:gen1:5142".to_owned(),
            "0:gen1:0:".to_owned(),
            format!("1:gen1:100:{digest}"),
            format!("2:gen1:5142:{digest}"),
            format!("3:gen.1:5142:{digest}"),
            format!("3::5142:{digest}"),
            format!("3:gen1:5142:{digest}:extra"),
            "3:gen1:5142:not-a-digest".to_owned(),
            format!("x:gen1:5142:{digest}"),
        ];
        for fields in bad {
            // Given: a manifest whose fields are unusable
            let mut manifest = super::MANIFEST_PREFIX.to_vec();
            manifest.extend_from_slice(fields.as_bytes());

            // When / Then: it is neither read as a value nor trusted for names
            assert_eq!(
                code(read(&manifest, &HashMap::new())).as_deref(),
                Some("CREDENTIAL_READ_FAILED"),
                "{fields}"
            );
            assert!(live_pieces(TARGET, &manifest).is_empty(), "{fields}");
        }
    }

    #[test]
    fn credential_orphans_are_every_piece_the_live_manifest_does_not_name() {
        // Given: a live two-piece manifest, a leftover older generation, and a
        // piece beyond the live count
        let live = layout(&long(MAX_BLOB_BYTES), "gen2");
        let live_names = live_pieces(TARGET, &live.primary);
        let stale = [
            piece_target(TARGET, "gen1", 0),
            piece_target(TARGET, "gen1", 1),
            piece_target(TARGET, "gen2", 9),
        ];
        let mut existing = live_names.clone();
        existing.extend(stale.clone());

        // When / Then: only the unnamed ones are swept
        assert_eq!(
            orphans(TARGET, Some(&live.primary), existing.clone()),
            stale
        );
        // A short value or no primary entry names nothing, so everything goes
        assert_eq!(
            orphans(TARGET, Some(&encode_blob("short")), existing.clone()),
            existing
        );
        assert_eq!(orphans(TARGET, None, existing.clone()), existing);
    }
}
