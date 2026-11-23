//! Pinned `gozd` daemon binary version and bundled binary discovery.

use std::path::PathBuf;

/// Pinned `goz` release version compatible with Ramen.
pub const PINNED_GOZ_VERSION: &str = "0.7.1";
/// Pinned `goz` Git tag.
pub const PINNED_GOZ_TAG: &str = "v0.7.1";

/// Locates the bundled pinned `gozd.exe` binary on disk.
///
/// Search order:
/// 1. `RAMEN_GOZD_PATH` environment override (when pointing to an existing file)
/// 2. Sibling `gozd.exe` beside the current executable (bundled installer/zip)
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

    None
}

/// Returns the bundled `gozd.exe` path or a clear error if it is missing.
pub fn require_gozd_binary() -> Result<PathBuf, String> {
    locate_gozd_binary().ok_or_else(|| {
        format!(
            "未找到随包捆绑的 gozd.exe ({PINNED_GOZ_TAG})，请重新安装 Ramen 或通过 RAMEN_GOZD_PATH 指定路径。"
        )
    })
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
}
