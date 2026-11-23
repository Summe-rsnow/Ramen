//! Ramen: Millisecond-class native Windows disk file search utility powered by
//! `goz` v0.7.1, Native Shell (NSH), and Bento.

pub mod dialogs;
pub mod goz;
pub mod shell;
pub mod store;
pub mod ui;
pub mod views;

use bento::AppMeta;
use nsh_base::theme;
use nsh_core::prelude::*;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const APP_META: AppMeta = AppMeta {
    name: "Ramen",
    version: APP_VERSION,
    tagline: "毫秒级原生 Windows 全盘文件极速搜索工具",
    github_repo: "Summe-rsnow/Ramen",
    license: "Apache-2.0",
    brand_mark: |size| ui::icons::brand_mark(size).into_element(),
};

/// The app's UI set (`nsh.json` `"rust"`).
#[native_module("ramen/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// The application's root component (`nsh.json` `"root"`).
#[component]
pub fn App() -> impl IntoElement {
    bento::shell::init_desktop_window(820.0, 540.0, 48.0);
    store::load();

    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .p_safe_area()
        .child(shell::screen())
}
