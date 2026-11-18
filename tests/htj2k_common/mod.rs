//! What the tests that compare exrs's HTJ2K and LJ2K chunks with OpenEXR's
//! share: the version of OpenJPH named in the codestream's comment.
//!
//! OpenJPH starts the main header of every codestream with a COM marker
//! segment "OpenJPH Ver X.Y.Z.". exrs's chunks say 0.33.0, the version the
//! htj2k crate ports. The installed OpenEXR 3.5.3, and the committed
//! fixtures it wrote, link OpenJPH 0.32.0 and say so. The comment is only
//! informative, so the tests compare chunks with the version text blanked
//! out, and check separately that each side names the version it should.
//! Everything else in the chunks is still compared.

#![allow(dead_code)]

use std::convert::TryInto;

/// The version exrs's codestreams name.
pub const EXRS_OPENJPH_VERSION: &str = "0.33.0";

/// The version the codestreams of the installed OpenEXR 3.5.3 and of the
/// committed fixtures name.
pub const OPENEXR_OPENJPH_VERSION: &str = "0.32.0";

const PREFIX: &[u8] = b"OpenJPH Ver ";

/// For a chunk coded as an HTJ2K or LJ2K codestream (magic number "HT" or
/// "HL"), the OpenJPH version its COM segment names, if it has one, and the
/// chunk with the bytes of that version replaced by `#`, so that two chunks
/// that differ only there compare equal. Any other chunk, such as one
/// stored raw, comes back unchanged, with no version.
pub fn without_openjph_version(chunk: &[u8]) -> (Option<String>, Vec<u8>) {
    let mut normalised = chunk.to_vec();
    let version = version_range(chunk).map(|range| {
        let version = String::from_utf8_lossy(&chunk[range.clone()]).into_owned();
        normalised[range].iter_mut().for_each(|byte| *byte = b'#');
        version
    });
    (version, normalised)
}

/// Where the version text ("X.Y.Z", without the final ".") of the
/// "OpenJPH Ver X.Y.Z." comment of the main header is.
fn version_range(chunk: &[u8]) -> Option<std::ops::Range<usize>> {
    if !(chunk.starts_with(b"HT") || chunk.starts_with(b"HL")) {
        return None;
    }
    let u16_at = |at: usize| Some(u16::from_be_bytes(chunk.get(at..at + 2)?.try_into().ok()?));
    let payload = u32::from_be_bytes(chunk.get(2..6)?.try_into().ok()?) as usize;
    let mut position = 6 + payload;
    if u16_at(position)? != 0xFF4F {
        return None;
    }
    position += 2;
    loop {
        let marker = u16_at(position)?;
        if marker == 0xFF90 {
            return None;
        }
        let end = position + 2 + usize::from(u16_at(position + 2)?);
        // COM: marker, length, registration (2 bytes), then the text
        let text = position + 6;
        if marker == 0xFF64 && chunk.get(text..end)?.starts_with(PREFIX) {
            let version = text + PREFIX.len()..end - 1;
            return (chunk[end - 1] == b'.' && !version.is_empty()).then_some(version);
        }
        position = end;
    }
}

/// How two chunks of one position, exrs's and OpenEXR's, differ, if they
/// do, after checking that each coded chunk names the OpenJPH version it
/// should and blanking out that version.
pub fn compare_normalised(ours: &[u8], theirs: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (our_version, ours_normalised) = without_openjph_version(ours);
    let (their_version, theirs_normalised) = without_openjph_version(theirs);
    check_version("exrs's", ours, our_version, EXRS_OPENJPH_VERSION)?;
    check_version("OpenEXR's", theirs, their_version, OPENEXR_OPENJPH_VERSION)?;
    Ok((ours_normalised, theirs_normalised))
}

fn check_version(
    whose: &str,
    chunk: &[u8],
    version: Option<String>,
    expected: &str,
) -> Result<(), String> {
    let coded = chunk.starts_with(b"HT") || chunk.starts_with(b"HL");
    match version {
        Some(version) if version == expected => Ok(()),
        Some(version) => Err(format!("{whose} chunk names OpenJPH {version}, not {expected}")),
        None if coded => Err(format!("{whose} chunk has no OpenJPH version comment")),
        None => Ok(()),
    }
}
