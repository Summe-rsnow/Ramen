//! Ramen brand mark, system tray icon, file-kind icon mapping, and Bento stroke icons.

use std::sync::OnceLock;

pub use bento::ui::icons::*;
use nsh_base::theme;
use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

use crate::goz::FileIconKind;

/// Embedded Ramen full-color brand mark SVG (`assets/logo-mark.svg`).
pub const BRAND_MARK: &str = include_str!("../../assets/logo-mark.svg");

/// Embedded 64×64 RGBA PNG used for the system tray icon (`assets/tray-icon.png`).
pub const TRAY_ICON_PNG: &[u8] = include_bytes!("../../assets/tray-icon.png");

/// Renders the full-color Ramen brand mark at `size` points.
pub fn brand_mark(size: f32) -> impl IntoElement {
    bento::ui::icons::brand_mark_svg(BRAND_MARK, size)
}

/// Returns the cached full-color 64×64 data URI for the system tray icon.
pub fn tray_icon_src() -> ImageSrc {
    static DATA_URI: OnceLock<ImageSrc> = OnceLock::new();
    DATA_URI
        .get_or_init(|| bento::ui::png_data_uri(TRAY_ICON_PNG))
        .clone()
}

/// Maps a [`FileIconKind`] to the matching 24×24 Bento stroke icon markup.
pub fn file_icon_markup(kind: FileIconKind) -> &'static str {
    match kind {
        FileIconKind::Folder => FOLDER,
        FileIconKind::Document => FILE_TEXT,
        FileIconKind::Image => IMAGE,
        FileIconKind::Video => VIDEO,
        FileIconKind::Audio => MUSIC,
        FileIconKind::Archive => ARCHIVE,
        FileIconKind::Code => CODE,
        FileIconKind::Executable => TERMINAL,
        FileIconKind::Other => FILE,
    }
}

/// Resolves the semantic icon color for a file row icon.
pub fn file_icon_color(kind: FileIconKind, selected: bool) -> AdaptiveColor {
    if selected {
        theme().accent_foreground
    } else if kind == FileIconKind::Folder {
        theme().primary
    } else {
        theme().muted_foreground
    }
}
