//! Pinned `gozd` daemon binary version, discovery, download, and SHA-256 verification.

use std::fs::{self, File};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// Pinned `goz` release version compatible with Ramen.
pub const PINNED_GOZ_VERSION: &str = "0.7.1";
/// Pinned `goz` Git tag.
pub const PINNED_GOZ_TAG: &str = "v0.7.1";
/// Official Windows x86_64 release archive URL for the pinned `goz` release.
pub const PINNED_GOZ_ZIP_URL: &str =
    "https://github.com/mustafaahci/goz/releases/download/v0.7.1/goz-v0.7.1-windows-x86_64.zip";
/// Expected lowercase SHA-256 hex digest of [`PINNED_GOZ_ZIP_URL`].
pub const PINNED_GOZ_ZIP_SHA256: &str =
    "7044c8d2938b7b684818140c86c61042969548b0c62d46faf68deed216460220";

/// Maximum archive payload accepted when downloading `gozd` (64 MiB).
const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);

/// Returns `%LOCALAPPDATA%\Ramen\goz\v0.7.1\gozd.exe` (or platform local data equivalent).
pub fn cached_gozd_path() -> Option<PathBuf> {
    let base = dirs::data_local_dir()
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))?;
    Some(
        base.join("Ramen")
            .join("goz")
            .join(PINNED_GOZ_TAG)
            .join("gozd.exe"),
    )
}

/// Locates an already-available pinned `gozd.exe` binary without hitting the network.
///
/// Search order:
/// 1. `RAMEN_GOZD_PATH` environment override (when pointing to an existing file)
/// 2. Sibling `gozd.exe` beside the current executable (bundled installer/zip)
/// 3. Cached download path returned by [`cached_gozd_path`]
pub fn locate_gozd_binary() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os("RAMEN_GOZD_PATH") {
        let candidate = PathBuf::from(override_path);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let bundled = dir.join("gozd.exe");
        if bundled.is_file() {
            return Some(bundled);
        }
    }

    cached_gozd_path().filter(|p| p.is_file())
}

/// Ensures the pinned `gozd.exe` binary exists locally, downloading and verifying it if needed.
///
/// Checks [`locate_gozd_binary`] first. If missing, downloads [`PINNED_GOZ_ZIP_URL`] via `ureq`,
/// verifies its SHA-256 matches [`PINNED_GOZ_ZIP_SHA256`], and atomically extracts `gozd.exe`
/// into [`cached_gozd_path`].
pub fn ensure_gozd_binary() -> Result<PathBuf, String> {
    if let Some(existing) = locate_gozd_binary() {
        return Ok(existing);
    }

    let dest = cached_gozd_path()
        .ok_or_else(|| "Unable to determine local application data directory".to_owned())?;

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .build()
        .into();

    let mut response = agent
        .get(PINNED_GOZ_ZIP_URL)
        .header(
            "User-Agent",
            &format!("Ramen/{}", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| format!("Failed to download gozd {PINNED_GOZ_TAG}: {e}"))?;

    let zip_bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_ARCHIVE_BYTES)
        .read_to_vec()
        .map_err(|e| format!("Failed to read gozd archive body: {e}"))?;

    extract_gozd_from_zip(&zip_bytes, PINNED_GOZ_ZIP_SHA256, &dest)?;
    Ok(dest)
}

/// Computes the lowercase hexadecimal SHA-256 digest of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

/// Verifies `zip_bytes` against `expected_sha256` and extracts `gozd.exe` atomically to `dest`.
pub(crate) fn extract_gozd_from_zip(
    zip_bytes: &[u8],
    expected_sha256: &str,
    dest: &Path,
) -> Result<(), String> {
    let actual_sha256 = sha256_hex(zip_bytes);
    if !actual_sha256.eq_ignore_ascii_case(expected_sha256.trim()) {
        return Err(format!(
            "SHA-256 mismatch for gozd archive: expected {}, got {actual_sha256}",
            expected_sha256.trim().to_ascii_lowercase()
        ));
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|e| format!("Invalid gozd zip archive: {e}"))?;

    let mut entry_index = None;
    for idx in 0..archive.len() {
        let entry = archive
            .by_index(idx)
            .map_err(|e| format!("Failed to inspect zip entry {idx}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let file_name = entry.name().rsplit(['/', '\\']).next().unwrap_or("");
        if file_name.eq_ignore_ascii_case("gozd.exe") {
            entry_index = Some(idx);
            break;
        }
    }

    let idx = entry_index.ok_or_else(|| "gozd.exe was not found inside the archive".to_owned())?;
    let mut entry = archive
        .by_index(idx)
        .map_err(|e| format!("Failed to open gozd.exe inside archive: {e}"))?;

    let parent = dest
        .parent()
        .ok_or_else(|| "Target gozd path has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;

    let tmp_name = format!(
        "gozd.exe.tmp.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let tmp_path = parent.join(tmp_name);

    let write_res = (|| -> io::Result<()> {
        let mut out = File::create(&tmp_path)?;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
        }
        out.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Failed to extract gozd.exe: {e}"));
    }

    if let Err(e) = replace_file_atomically(&tmp_path, dest) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!(
            "Failed to place gozd.exe at {}: {e}",
            dest.display()
        ));
    }

    Ok(())
}

fn replace_file_atomically(tmp_path: &Path, dest: &Path) -> io::Result<()> {
    match fs::rename(tmp_path, dest) {
        Ok(()) => Ok(()),
        Err(_err) if dest.exists() => {
            fs::remove_file(dest)?;
            fs::rename(tmp_path, dest)
        }
        Err(err) => Err(err),
    }
}

/// Returns `true` if a `Response::Hello` `server` banner matches [`PINNED_GOZ_VERSION`].
///
/// Accepts banners formatted like `"gozd 0.7.1"` or `"gozd v0.7.1"` (with optional trailing build
/// metadata or whitespace) and rejects mismatched versions such as `"gozd 0.7.0"` or `"gozd 0.7.10"`.
pub fn server_matches_pinned(banner: &str) -> bool {
    let mut parts = banner.split_whitespace();
    let Some(name) = parts.next() else {
        return false;
    };
    if !name.eq_ignore_ascii_case("gozd") {
        return false;
    }
    let Some(raw_ver) = parts.next() else {
        return false;
    };
    let ver = raw_ver
        .strip_prefix('v')
        .or_else(|| raw_ver.strip_prefix('V'))
        .unwrap_or(raw_ver);
    let base_ver = ver.split('+').next().unwrap_or(ver);
    base_ver == PINNED_GOZ_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an uncompressed (STORE) single-file ZIP archive in memory so we can test
    /// SHA-256 verification and atomic extraction without requiring external fixtures.
    fn make_stored_zip(entry_name: &str, data: &[u8]) -> Vec<u8> {
        let crc = crc32(data);
        let name_bytes = entry_name.as_bytes();
        let len = data.len() as u32;
        let name_len = name_bytes.len() as u16;

        let mut out = Vec::new();
        // Local file header
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method = STORE
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);

        let cd_offset = out.len() as u32;
        // Central directory header
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version made by
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method = STORE
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out.extend_from_slice(&0u16.to_le_bytes()); // disk number
        out.extend_from_slice(&0u16.to_le_bytes()); // internal attr
        out.extend_from_slice(&0u32.to_le_bytes()); // external attr
        out.extend_from_slice(&0u32.to_le_bytes()); // local header offset
        out.extend_from_slice(name_bytes);

        let cd_size = (out.len() as u32) - cd_offset;
        // End of central directory
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // disk
        out.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
        out.extend_from_slice(&1u16.to_le_bytes()); // entries on disk
        out.extend_from_slice(&1u16.to_le_bytes()); // total entries
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in bytes {
            crc ^= u32::from(b);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    #[test]
    fn server_banner_matching() {
        assert!(server_matches_pinned("gozd 0.7.1"));
        assert!(server_matches_pinned("gozd v0.7.1"));
        assert!(server_matches_pinned("  GOZD   0.7.1  "));
        assert!(server_matches_pinned("gozd 0.7.1+git"));
        assert!(server_matches_pinned("gozd 0.7.1 (windows)"));

        assert!(!server_matches_pinned(""));
        assert!(!server_matches_pinned("0.7.1"));
        assert!(!server_matches_pinned("gozd"));
        assert!(!server_matches_pinned("gozd 0.7.0"));
        assert!(!server_matches_pinned("gozd 0.7.10"));
        assert!(!server_matches_pinned("gozd 0.7.1-rc1"));
        assert!(!server_matches_pinned("everything 0.7.1"));
    }

    #[test]
    fn extract_gozd_verifies_sha256_and_writes_atomically() {
        let payload = b"MZ-fake-gozd-binary";
        let zip_bytes = make_stored_zip("goz-v0.7.1-windows-x86_64/gozd.exe", payload);
        let digest = sha256_hex(&zip_bytes);

        let dir = std::env::temp_dir().join(format!(
            "ramen-version-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let dest = dir.join("gozd.exe");

        // Wrong digest must fail before writing anything.
        let bad_err = extract_gozd_from_zip(
            &zip_bytes,
            "0000000000000000000000000000000000000000000000000000000000000000",
            &dest,
        )
        .unwrap_err();
        assert!(bad_err.contains("SHA-256 mismatch"), "{bad_err}");
        assert!(!dest.exists());

        // Valid digest extracts atomically (case-insensitive hex).
        extract_gozd_from_zip(&zip_bytes, &digest.to_ascii_uppercase(), &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), payload);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_gozd_errors_when_binary_missing_in_zip() {
        let zip_bytes = make_stored_zip("goz.exe", b"cli-only");
        let digest = sha256_hex(&zip_bytes);
        let dir = std::env::temp_dir().join(format!(
            "ramen-version-missing-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let dest = dir.join("gozd.exe");

        let err = extract_gozd_from_zip(&zip_bytes, &digest, &dest).unwrap_err();
        assert!(err.contains("not found"), "{err}");
        assert!(!dest.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
