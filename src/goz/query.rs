//! Search category filters, `goz` query composition, path splitting, and file icon detection.

/// Quick category filter chip applied on top of the user's search text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FileCategory {
    #[default]
    All,
    Folders,
    Documents,
    Images,
    Videos,
    Audio,
    Archives,
    Code,
}

impl FileCategory {
    /// All categories in toolbar chip display order.
    pub const ALL: [FileCategory; 8] = [
        Self::All,
        Self::Folders,
        Self::Documents,
        Self::Images,
        Self::Videos,
        Self::Audio,
        Self::Archives,
        Self::Code,
    ];

    /// Short display label for filter chips and accessibility names.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Folders => "Folders",
            Self::Documents => "Documents",
            Self::Images => "Images",
            Self::Videos => "Videos",
            Self::Audio => "Audio",
            Self::Archives => "Archives",
            Self::Code => "Code",
        }
    }

    /// Returns the `goz` query filter expression for this category (`None` for [`Self::All`]).
    pub fn goz_filter(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Folders => Some("folder:"),
            Self::Documents => Some(
                "ext:pdf;doc;docx;xls;xlsx;xlsm;ppt;pptx;txt;md;rtf;csv;tsv;odt;ods;odp;epub;mobi;pages;numbers;key",
            ),
            Self::Images => Some("pic:"),
            Self::Videos => Some("video:"),
            Self::Audio => Some("audio:"),
            Self::Archives => Some("zip:"),
            Self::Code => Some(
                "ext:rs;ts;tsx;js;jsx;mjs;cjs;py;go;c;cc;cpp;cxx;h;hpp;hxx;cs;java;kt;kts;swift;rb;php;lua;zig;sh;bash;zsh;ps1;psm1;bat;cmd;html;htm;css;scss;sass;less;json;jsonc;toml;yaml;yml;xml;sql;graphql;proto",
            ),
        }
    }
}

/// Composes the final `goz` query string from the user's search input and selected [`FileCategory`].
pub fn compose_query(user_input: &str, category: FileCategory) -> String {
    let trimmed = user_input.trim();
    match category.goz_filter() {
        None => trimmed.to_owned(),
        Some(filter) if trimmed.is_empty() => filter.to_owned(),
        Some(filter) => format!("{filter} {trimmed}"),
    }
}

/// Splits a full file or directory path into `(parent_dir, file_name)`.
///
/// Preserves drive root backslashes (`"C:\\main.rs"` -> `("C:\\", "main.rs")`) and Unix root
/// slashes (`"/etc"` -> `("/", "etc")`). Root-only paths (`"C:\\"`, `"/"`) or bare filenames
/// (`"readme.md"`) return `("", full_path)`.
pub fn split_path(full_path: &str) -> (&str, &str) {
    let normalized = full_path.trim_end_matches(['\\', '/']);
    // Bare root like "/" or "C:\" or "\\?\Volume{...}\"
    if normalized.is_empty() {
        return ("", full_path);
    }
    if is_drive_root(full_path) {
        return ("", full_path);
    }

    match normalized.rfind(['\\', '/']) {
        Some(idx) => {
            let name = &normalized[idx + 1..];
            let before = &normalized[..idx];
            let parent = if before.is_empty() {
                // Unix root child like "/etc" -> parent "/"
                &normalized[..1]
            } else if before.len() == 2 && before.as_bytes()[1] == b':' {
                // Drive root child like "C:\main.rs" -> keep trailing "\" as "C:\"
                &normalized[..=idx]
            } else {
                before
            };
            (parent, name)
        }
        None => ("", normalized),
    }
}

fn is_drive_root(path: &str) -> bool {
    let bytes = path.as_bytes();
    match bytes {
        [drive, b':'] => drive.is_ascii_alphabetic(),
        [drive, b':', b'\\' | b'/'] => drive.is_ascii_alphabetic(),
        _ => false,
    }
}

/// Semantic icon classification for search result rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileIconKind {
    Folder,
    Document,
    Image,
    Video,
    Audio,
    Archive,
    Code,
    Executable,
    Other,
}

/// Classifies a filename (or path) and directory flag into a [`FileIconKind`].
pub fn detect_file_kind(name: &str, is_dir: bool) -> FileIconKind {
    if is_dir {
        return FileIconKind::Folder;
    }

    let base = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let lower_base = base.to_ascii_lowercase();
    if matches!(
        lower_base.as_str(),
        "dockerfile" | "makefile" | "cmakelists.txt" | "cargo.lock"
    ) {
        return FileIconKind::Code;
    }

    let Some((_, ext)) = base.rsplit_once('.') else {
        return FileIconKind::Other;
    };
    if ext.is_empty() {
        return FileIconKind::Other;
    }

    match ext.to_ascii_lowercase().as_str() {
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "xlsm" | "ppt" | "pptx" | "txt" | "md"
        | "rtf" | "csv" | "tsv" | "odt" | "ods" | "odp" | "epub" | "mobi" | "pages"
        | "numbers" | "key" | "log" => FileIconKind::Document,

        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "tif" | "tiff"
        | "avif" | "heic" | "heif" | "psd" | "jxl" | "cur" => FileIconKind::Image,

        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg"
        | "3gp" | "m2ts" | "mts" | "vob" => FileIconKind::Video,

        "mp3" | "wav" | "flac" | "aac" | "ogg" | "oga" | "opus" | "m4a" | "wma" | "aif"
        | "aiff" | "mid" | "midi" => FileIconKind::Audio,

        "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab" | "iso"
        | "jar" => FileIconKind::Archive,

        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "py" | "go" | "c" | "cc"
        | "cpp" | "cxx" | "h" | "hpp" | "hxx" | "cs" | "java" | "kt" | "kts" | "swift"
        | "rb" | "php" | "lua" | "zig" | "sh" | "bash" | "zsh" | "ps1" | "psm1" | "html"
        | "htm" | "css" | "scss" | "sass" | "less" | "json" | "jsonc" | "toml" | "yaml"
        | "yml" | "xml" | "sql" | "graphql" | "proto" | "gitignore" | "env" => FileIconKind::Code,

        "exe" | "msi" | "msix" | "bat" | "cmd" | "com" | "dll" | "sys" | "scr" | "vbs"
        | "lnk" => FileIconKind::Executable,

        _ => FileIconKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goz_core::query::parse_query;

    #[test]
    fn compose_query_combines_category_and_input() {
        assert_eq!(compose_query("  cargo.toml ", FileCategory::All), "cargo.toml");
        assert_eq!(compose_query("", FileCategory::All), "");
        assert_eq!(compose_query("", FileCategory::Folders), "folder:");
        assert_eq!(
            compose_query("target", FileCategory::Folders),
            "folder: target"
        );
        assert_eq!(compose_query("icon", FileCategory::Images), "pic: icon");
        assert_eq!(compose_query("trailer", FileCategory::Videos), "video: trailer");
        assert_eq!(compose_query("track", FileCategory::Audio), "audio: track");
        assert_eq!(compose_query("backup", FileCategory::Archives), "zip: backup");
    }

    #[test]
    fn all_category_filters_parse_cleanly_in_goz_core() {
        for cat in FileCategory::ALL {
            let composed = compose_query("report", cat);
            parse_query(&composed).unwrap_or_else(|e| {
                panic!("category {:?} produced invalid goz query '{composed}': {e}", cat)
            });
            let empty_only = compose_query("", cat);
            parse_query(&empty_only).unwrap_or_else(|e| {
                panic!("empty category {:?} query '{empty_only}' failed: {e}", cat)
            });
        }
    }

    #[test]
    fn split_path_handles_windows_unc_unix_and_roots() {
        assert_eq!(split_path(r"C:\src\main.rs"), (r"C:\src", "main.rs"));
        assert_eq!(split_path(r"C:\main.rs"), (r"C:\", "main.rs"));
        assert_eq!(split_path(r"C:\"), ("", r"C:\"));
        assert_eq!(split_path("C:"), ("", "C:"));
        assert_eq!(split_path(r"\\server\share\notes.txt"), (r"\\server\share", "notes.txt"));
        assert_eq!(split_path("readme.md"), ("", "readme.md"));
        assert_eq!(split_path("/usr/local/bin/rg"), ("/usr/local/bin", "rg"));
        assert_eq!(split_path("/etc"), ("/", "etc"));
        assert_eq!(split_path("/"), ("", "/"));
    }

    #[test]
    fn detect_file_kind_classifies_dirs_and_extensions() {
        assert_eq!(detect_file_kind("src", true), FileIconKind::Folder);
        assert_eq!(detect_file_kind("folder.png", true), FileIconKind::Folder);
        assert_eq!(detect_file_kind(r"C:\app\main.RS", false), FileIconKind::Code);
        assert_eq!(detect_file_kind("Dockerfile", false), FileIconKind::Code);
        assert_eq!(detect_file_kind("notes.PDF", false), FileIconKind::Document);
        assert_eq!(detect_file_kind("hero.webp", false), FileIconKind::Image);
        assert_eq!(detect_file_kind("clip.MKV", false), FileIconKind::Video);
        assert_eq!(detect_file_kind("song.flac", false), FileIconKind::Audio);
        assert_eq!(detect_file_kind("release.zip", false), FileIconKind::Archive);
        assert_eq!(detect_file_kind("gozd.EXE", false), FileIconKind::Executable);
        assert_eq!(detect_file_kind(r"C:\foo.bar\README", false), FileIconKind::Other);
        assert_eq!(detect_file_kind("noext", false), FileIconKind::Other);
    }
}
