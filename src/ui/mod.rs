//! Ramen's styled UI layer: re-exports shared `bento::ui` controls and `bento::format`
//! formatters alongside Ramen-specific icons, search bar, column headers, and virtualized rows.

pub mod components;
pub mod icons;

pub use bento::format::*;
pub use bento::ui::*;
pub use components::{
    clickable_hotkey_badge, inline_notice_banner, result_row, results_table_header, scope_pill,
    search_toolbar_input,
};
