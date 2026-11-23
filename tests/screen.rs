//! Headless UI integration tests covering Ramen's shell, onboarding flow,
//! seeded search workspace, filtering, sorting, keyboard navigation, context menu,
//! virtualized scrolling, and modal dialogs.

use std::time::Duration;

use nsh_app_ramen::goz::{
    DaemonServiceState, DaemonStatus, ResultItem, VolumePhase, VolumeStatus,
};
use nsh_app_ramen::{shell, store};
use nsh_base::theme;
use nsh_core::platform::{Idiom, Os};
use nsh_core::prelude::*;
use nsh_engine::{EngineConfig, Points, Root, Size};
use nsh_testing::Headless;

fn start_desktop<C: Component>(width: f32, height: f32) -> Result<Headless, String>
where
    C::Props: Default,
{
    let mut cfg = EngineConfig::new(Root::of::<C>());
    cfg.viewport = Size::new(Points(width), Points(height));
    cfg.environment.os = Os::Windows;
    cfg.environment.idiom = Idiom::Desktop;
    Headless::start(cfg).map_err(|e| e.to_string())
}

fn sample_status(volumes_incomplete: bool, metadata_pending: bool) -> DaemonStatus {
    DaemonStatus {
        volumes: vec![VolumeStatus {
            guid: r"\\?\Volume{00000000-0000-0000-0000-000000000001}\".to_owned(),
            mounts: vec![r"C:\".to_owned()],
            phase: if volumes_incomplete {
                VolumePhase::Rescanning
            } else {
                VolumePhase::Live
            },
            entries: 420_000,
            generation: 42,
            metadata_pending,
            journal_max_bytes: Some(32 * 1024 * 1024),
            has_orphans: false,
            has_parent_cycle: false,
            names_waiting: 0,
            placeholders_created: 0,
            delete_of_unknown: 0,
            stale_slots: 0,
            link_reconciles_dropped: 0,
            memory: None,
            columns: Vec::new(),
        }],
        volumes_incomplete,
        process_working_set: 24 * 1024 * 1024,
        process_private_bytes: 16 * 1024 * 1024,
    }
}

fn sample_items() -> Vec<ResultItem> {
    const FT: i64 = 133_500_000_000_000_000;
    vec![
        ResultItem {
            path: r"C:\repo\src\main.rs".to_owned(),
            size: Some(4096),
            mtime_ft: Some(FT),
            is_dir: false,
            ..Default::default()
        },
        ResultItem {
            path: r"C:\repo\src\lib.rs".to_owned(),
            size: Some(2048),
            mtime_ft: Some(FT + 10_000_000),
            is_dir: false,
            ..Default::default()
        },
        ResultItem {
            path: r"C:\repo\docs\README.md".to_owned(),
            size: Some(1536),
            mtime_ft: Some(FT + 20_000_000),
            is_dir: false,
            ..Default::default()
        },
        ResultItem {
            path: r"C:\repo\assets\logo.png".to_owned(),
            size: Some(8192),
            mtime_ft: Some(FT + 30_000_000),
            is_dir: false,
            ..Default::default()
        },
        ResultItem {
            path: r"C:\repo\UpperCaseFile.RS".to_owned(),
            size: Some(512),
            mtime_ft: Some(FT + 40_000_000),
            is_dir: false,
            ..Default::default()
        },
        ResultItem {
            path: r"C:\repo\src".to_owned(),
            size: None,
            mtime_ft: None,
            is_dir: true,
            ..Default::default()
        },
    ]
}

fn seeded_shell_wrapper() -> impl IntoElement {
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(shell::screen())
}

#[component]
fn OnboardingNotInstalledApp() -> impl IntoElement {
    store::seed_offline_for_test(DaemonServiceState::NotInstalled);
    seeded_shell_wrapper()
}

#[component]
fn OnboardingStoppedApp() -> impl IntoElement {
    store::seed_offline_for_test(DaemonServiceState::Stopped);
    seeded_shell_wrapper()
}

#[component]
fn SeededWorkspaceApp() -> impl IntoElement {
    store::seed_results_for_test(
        sample_items(),
        sample_status(true, true),
        false,
        true,
        true,
    );
    seeded_shell_wrapper()
}

#[component]
fn LargeResultsApp() -> impl IntoElement {
    let items = (0..120)
        .map(|i| ResultItem {
            path: format!(r"C:\bulk\item_{i:03}.txt"),
            size: Some((i as u64 + 1) * 1024),
            mtime_ft: Some(133_500_000_000_000_000 + (i as i64) * 10_000_000),
            is_dir: false,
            ..Default::default()
        })
        .collect();
    store::seed_results_for_test(items, sample_status(false, false), true, false, false);
    seeded_shell_wrapper()
}

#[test]
fn app_shell_renders_titlebar_order_theme_and_dialogs() -> Result<(), String> {
    let mut app = start_desktop::<nsh_app_ramen::App>(1040.0, 680.0)?;
    app.wait_for("Ramen")?;
    assert!(app.screen().shows("v1.0.0"));

    // Verify title-bar right-hand controls place action icons left of the theme switcher
    let layout = app.screen().layout_nodes();
    let refresh_id = app.screen().id("role:button[刷新或重新连接]")?;
    let settings_id = app.screen().id("role:button[设置与快捷键]")?;
    let about_id = app.screen().id("role:button[关于]")?;
    let theme_id = app.screen().id("role:button[跟随系统]")?;
    let x_of = |id| -> Result<f32, String> {
        layout
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.bounds.origin.x.0)
            .ok_or_else(|| format!("missing layout bounds for {id:?}"))
    };
    assert!(x_of(refresh_id)? < x_of(settings_id)?);
    assert!(x_of(settings_id)? < x_of(about_id)?);
    assert!(x_of(about_id)? < x_of(theme_id)?);

    // Open Settings & Service dialog from title bar, inspect controls, then close
    app.press("label:设置与快捷键")?;
    app.wait_for("设置与服务")?;
    assert!(app.screen().shows("全局召唤快捷键"));
    assert!(app.screen().shows("Alt+Space"));
    assert!(app.screen().shows("Ctrl+Shift+Space"));
    assert!(app.screen().shows("Ctrl+Alt+F"));
    assert!(app.screen().shows("每次询问"));
    assert!(app.screen().shows("最小化到托盘"));
    assert!(app.screen().shows("直接退出应用"));
    assert!(app.screen().shows("文件变化时自动刷新搜索结果"));
    assert!(app.screen().shows("后台索引服务 (gozd v0.7.1)"));
    assert!(app.screen().shows("安装 / 升级 goz v0.7.1"));

    app.press("关闭")?;
    app.wait_for_gone("设置与服务", Duration::from_secs(2))?;

    // Open Settings via Mod+, and dismiss via Escape
    app.press_key("mod+,")?;
    app.wait_for("设置与服务")?;
    app.press_key("escape")?;
    app.wait_for_gone("设置与服务", Duration::from_secs(2))?;

    // Open About dialog from brand pill
    app.press("label:关于 Ramen")?;
    app.wait_for("版本更新")?;
    assert!(app.screen().shows("Summe-rsnow/Ramen"));
    assert!(app.screen().shows("v1.0.0"));
    app.press("关闭")?;
    app.wait_for_gone("版本更新", Duration::from_secs(2))?;

    // Open Search Syntax quick reference dialog
    app.press("label:搜索语法")?;
    app.wait_for("goz 搜索语法速查")?;
    assert!(app.screen().shows("*.rs"));
    assert!(app.screen().shows("folder:"));
    assert!(app.screen().shows("ext:pdf;docx"));
    assert!(app.screen().shows("size:>10mb"));
    assert!(app.screen().shows("dm:today"));
    assert!(app.screen().shows("regex:"));
    assert!(app.screen().shows("!exclude"));
    app.press("关闭")?;
    app.wait_for_gone("goz 搜索语法速查", Duration::from_secs(2))?;

    // Switch themes cleanly
    app.press("label:深色模式")?;
    assert!(app.screen().shows("Ramen"));
    app.press("label:浅色模式")?;
    assert!(app.screen().shows("Ramen"));

    Ok(())
}

#[test]
fn offline_onboarding_shows_not_installed_and_stopped_actions() -> Result<(), String> {
    let mut not_installed = start_desktop::<OnboardingNotInstalledApp>(1040.0, 680.0)?;
    not_installed.wait_for("安装并启用全盘高速索引服务")?;
    assert!(not_installed
        .screen()
        .shows("一键安装并启动服务 (goz v0.7.1)"));
    assert!(not_installed.screen().shows("重新检测"));
    assert!(not_installed.screen().shows("打开服务设置…"));

    let mut stopped = start_desktop::<OnboardingStoppedApp>(1040.0, 680.0)?;
    stopped.wait_for("索引服务当前已停止")?;
    assert!(stopped.screen().shows("启动服务"));
    assert!(stopped.screen().shows("重新检测"));

    Ok(())
}

#[test]
fn seeded_search_filter_sort_mismatch_selection_and_context_menu() -> Result<(), String> {
    let mut app = start_desktop::<SeededWorkspaceApp>(1040.0, 680.0)?;
    app.wait_for("main.rs")?;

    // Version mismatch banner + status bar indicators
    assert!(app.screen().shows("升级到 goz v0.7.1"));
    assert!(app.screen().shows("索引构建中（结果可能不完整）"));
    assert!(app.screen().shows("正在补充大小与时间…"));
    assert!(app.screen().shows("6 项"));
    assert!(app.screen().shows("Alt+Space"));

    // Filter chips: Folders -> Code -> All
    app.press("id:category-filters >> 文件夹")?;
    app.wait_for("1 项")?;
    assert!(!app.screen().shows("main.rs"));

    app.press("id:category-filters >> 代码")?;
    app.wait_for("3 项")?;
    assert!(app.screen().shows("main.rs"));
    assert!(app.screen().shows("lib.rs"));
    assert!(app.screen().shows("UpperCaseFile.RS"));
    assert!(!app.screen().shows("README.md"));

    app.press("id:category-filters >> 全部")?;
    app.wait_for("6 项")?;

    // Search query + Aa match-case toggle
    app.type_text("label:搜索全盘文件", "uppercase")?;
    app.wait_for("1 项")?;
    assert!(app.screen().shows("UpperCaseFile.RS"));

    app.press("label:区分大小写")?;
    app.wait_for("没有匹配的文件或目录")?;
    assert!(app.screen().shows("0 项"));

    app.press("label:区分大小写")?;
    app.wait_for("UpperCaseFile.RS")?;

    app.press("label:清空搜索")?;
    app.wait_for("6 项")?;

    // Sortable headers keep result count intact and toggle cleanly
    app.press("按大小排序")?;
    app.press("按名称排序")?;
    assert!(app.screen().shows("6 项"));

    // Select a row directly and copy full path via Mod+C
    app.press(r"label:C:\repo\src\main.rs")?;
    app.press_key("mod+c")?;
    assert_eq!(app.screen().clipboard(), Some(r"C:\repo\src\main.rs"));

    // Keyboard down navigation advances selection to the next sorted row
    app.press_key("down")?;
    app.press_key("mod+c")?;
    assert_eq!(app.screen().clipboard(), Some(r"C:\repo\docs\README.md"));

    // Right-click context menu: verify actions, Copy Name, and Scope Directory
    app.right_click(r"label:C:\repo\src\main.rs")?;
    let menu = app.menu().ok_or("expected row context menu")?;
    let labels: Vec<&str> = menu.iter().map(|entry| entry.label.as_str()).collect();
    for expected in [
        "打开",
        "在资源管理器中显示",
        "复制完整路径",
        "复制文件名",
        "复制所在目录",
        "仅在此目录搜索",
    ] {
        assert!(
            labels.contains(&expected),
            "missing context menu item {expected:?} in {labels:?}"
        );
    }

    app.choose_menu_item("复制文件名")?;
    assert_eq!(app.screen().clipboard(), Some("main.rs"));

    app.right_click(r"label:C:\repo\src\main.rs")?;
    app.choose_menu_item("仅在此目录搜索")?;
    app.wait_for(r"目录: C:\repo\src")?;
    app.wait_for("3 项")?;
    assert!(!app.screen().shows("README.md"));

    app.press("label:清除目录范围")?;
    app.wait_for_gone(r"目录: C:\repo\src", Duration::from_secs(2))?;
    app.wait_for("6 项")?;

    // Status bar clickable hotkey badge opens Settings with connected daemon telemetry + uninstall confirm
    app.press("label:配置全局快捷键")?;
    app.wait_for("设置与服务")?;
    assert!(app.screen().shows("进程内存占用"));
    assert!(app.screen().shows("工作集 24.0 MB · 私有提交 16.0 MB"));
    app.press("卸载服务…")?;
    app.wait_for("确认卸载 goz 索引服务")?;
    app.press("取消")?;
    app.wait_for_gone("确认卸载 goz 索引服务", Duration::from_secs(2))?;
    app.press("关闭")?;
    app.wait_for_gone("设置与服务", Duration::from_secs(2))?;

    Ok(())
}

#[test]
fn virtual_list_scrolls_and_recycles_many_results() -> Result<(), String> {
    let mut app = start_desktop::<LargeResultsApp>(1040.0, 680.0)?;
    app.wait_for("120 项")?;
    app.wait_for("item_000.txt")?;

    app.scroll("id:results-list", Points(4500.0))?;
    app.wait_for("item_119.txt")?;

    Ok(())
}
