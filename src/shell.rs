//! Desktop application shell for Ramen:
//! - 48px hidden title bar (`bento::shell::title_bar_shell`) with brand lockup, live daemon
//!   status summary, refresh/settings/about buttons, and right-aligned `theme_mode_segmented`
//! - Window placement memory (`remember_window_placement`), global summon hotkey (`bind_global_hotkey`),
//!   close-to-tray handling (`bind_close_handler` + `confirm_close_dialog`), and system tray
//! - Workspace keyboard navigation (`up`, `down`, `pageup`, `pagedown`, `enter`, `mod+enter`,
//!   `mod+c`, `mod+f`/`mod+l`, `mod+,`, `escape`)

use bento::hotkey::{bind_global_hotkey, toggle_window_visibility};
use bento::shell::{
    about_icon_button, autostart_tray_item, bind_close_handler, close_behavior_submenu,
    confirm_close_dialog, remember_window_placement, save_window_placement,
    title_bar_autostart_toggle, title_bar_brand_clickable, title_bar_right_with_theme,
    title_bar_shell, CloseBehavior, DEFAULT_WINDOW_PLACEMENT_KEY,
};
use nsh_base::theme;
use nsh_core::element::desktop::{
    current_window, on_app_quit, quit, set_tray, tray, MenuItem, WindowRef,
};
use nsh_core::keymap::When;
use nsh_core::prelude::*;

use crate::dialogs;
use crate::goz::PINNED_GOZ_VERSION;
use crate::store::{self, DaemonUiState};
use crate::ui::{
    format_count, icon_action_spinning_with_tip, icon_action_with_tip, icons, status_badge,
    IconKind, ValueTone,
};
use crate::views;

/// Root desktop screen of Ramen.
pub fn screen() -> impl IntoElement {
    let win = current_window();
    let close_confirm_open = signal(false);
    let search_input_ref = element_ref();
    let results_list_ref = element_ref();
    let search_focused = signal(false);
    let search_has_selection = signal(false);

    // Persist and restore desktop window size & screen position
    remember_window_placement(win.clone(), 820.0, 540.0);

    // System-wide summon shortcut (default: Alt+Space, enabled)
    let win_hotkey = win.clone();
    let search_ref_hotkey = search_input_ref;
    bind_global_hotkey(
        store::global_hotkey,
        store::set_hotkey_status,
        move || {
            let was_visible_focused = untracked(|| win_hotkey.state())
                .is_some_and(|st| st.visible && st.focused && !st.minimized);
            toggle_window_visibility(&win_hotkey);
            if !was_visible_focused {
                search_ref_hotkey.focus();
            }
        },
    );

    // Background SearchEngine event poller (32ms) and floating notice auto-dismiss
    store::start_engine_poller();
    store::notices().install_auto_dismiss();
    on_app_quit(|| {
        store::shutdown_engine();
        std::future::ready(())
    });
    cleanup(store::shutdown_engine);

    // Intercept close button according to user's CloseBehavior preference
    bind_close_handler(win.clone(), store::close_behavior_untracked, move || {
        close_confirm_open.set(true);
    });

    // System tray synchronized with CloseBehavior and live daemon state
    install_system_tray(&win, search_input_ref);

    let any_dialog_open = move || {
        close_confirm_open.get()
            || store::about_open()
            || store::settings_open()
            || store::syntax_help_open()
            || store::uninstall_confirm_open()
    };

    let any_dialog_open_untracked = move || {
        close_confirm_open.get_untracked()
            || store::about_open()
            || store::settings_open()
            || store::syntax_help_open()
            || store::uninstall_confirm_open()
    };

    // Global in-window shortcuts
    on_key_with(
        "mod+,",
        KeyOptions {
            when: When::Always,
            ..Default::default()
        },
        move |_| {
            if !close_confirm_open.get_untracked() {
                store::set_settings_open(true);
            }
        },
    );

    for focus_key in ["mod+f", "mod+l"] {
        let input_ref = search_input_ref;
        on_key_with(
            focus_key,
            KeyOptions {
                when: When::Always,
                ..Default::default()
            },
            move |_| {
                if !any_dialog_open_untracked() {
                    input_ref.focus();
                    let len = store::query_untracked().encode_utf16().count() as u32;
                    input_ref.set_selection(0, len);
                }
            },
        );
    }

    let win_escape = win.clone();
    on_key_with(
        "escape",
        KeyOptions {
            when: When::Always,
            ..Default::default()
        },
        move |_| {
            if close_confirm_open.get_untracked() {
                close_confirm_open.set(false);
            } else if store::dismiss_top_dialog() {
                // Topmost modal dismissed
            } else if !store::query_untracked().is_empty() {
                store::clear_query();
            } else if store::selected_index_untracked().is_some() {
                store::set_selected_index(None);
            } else if store::scope().is_some() {
                store::set_scope(None);
            } else if store::close_behavior_untracked() == CloseBehavior::Tray {
                save_window_placement(&win_escape, DEFAULT_WINDOW_PLACEMENT_KEY, 820.0, 540.0);
                win_escape.hide();
            }
        },
    );

    // Search results navigation & row action shortcuts (active only when connected and no modal is open)
    effect(move || {
        if any_dialog_open() || !store::is_connected() {
            return;
        }

        let move_and_scroll = move |delta: isize| {
            if let Some(next_idx) = store::move_selection(delta) {
                results_list_ref.scroll_to_index(
                    next_idx as u32,
                    percentage(0.5),
                    px(0.0),
                    false,
                );
            }
        };

        for (key, delta) in [("up", -1), ("down", 1), ("pageup", -12), ("pagedown", 12)] {
            on_key_with(
                key,
                KeyOptions {
                    when: When::Always,
                    repeat: true,
                    ..Default::default()
                },
                move |_| move_and_scroll(delta),
            );
        }

        on_key_with(
            "enter",
            KeyOptions {
                when: When::NotEditing,
                ..Default::default()
            },
            |_| {
                if let Some(row) = store::selected_or_first_row() {
                    store::open_entry(&row);
                }
            },
        );

        on_key_with(
            "mod+enter",
            KeyOptions {
                when: When::Always,
                ..Default::default()
            },
            |_| {
                if let Some(row) = store::selected_or_first_row() {
                    store::reveal_entry(&row);
                }
            },
        );

        if !(search_focused.get() && search_has_selection.get()) {
            on_key_with(
                "mod+c",
                KeyOptions {
                    when: When::Always,
                    ..Default::default()
                },
                |_| {
                    if let Some(row) = store::selected_row() {
                        store::copy_path(&row.item.path, "路径");
                    }
                },
            );
        }
    });

    view()
        .flex_1()
        .relative()
        .bg(move || theme().background)
        .child(title_bar())
        .child(views::search_workspace(
            search_input_ref,
            results_list_ref,
            search_focused,
            search_has_selection,
        ))
        // Floating notice toast (hidden while modal dialogs are open)
        .child(store::notices().overlay(move || !any_dialog_open()))
        // Modal overlays (later items sit above earlier items)
        .child(dialogs::about_dialog_overlay())
        .child(dialogs::settings_dialog_overlay())
        .child(dialogs::syntax_help_overlay())
        .child(dialogs::confirm_uninstall_overlay())
        .child(when(
            move || close_confirm_open.get(),
            move || {
                confirm_close_dialog(
                    close_confirm_open,
                    win.clone(),
                    crate::APP_META.name,
                    store::set_close_behavior,
                )
            },
        ))
}

fn install_system_tray(win: &WindowRef, search_input_ref: ElementRef) {
    let win_tray = win.clone();
    effect(move || {
        let behavior = store::close_behavior();
        let tooltip = match store::daemon_state() {
            DaemonUiState::Connected { .. } => {
                let entries = store::total_indexed_entries();
                if entries > 0 {
                    format!(
                        "Ramen v{} — goz v{PINNED_GOZ_VERSION} ({} 项索引)",
                        crate::APP_VERSION,
                        format_count(entries)
                    )
                } else {
                    format!("Ramen v{} — 已连接 goz v{PINNED_GOZ_VERSION}", crate::APP_VERSION)
                }
            }
            _ => format!("Ramen v{} — Windows 全盘文件极速搜索", crate::APP_VERSION),
        };

        let win_click = win_tray.clone();
        let win_toggle = win_tray.clone();
        let win_settings = win_tray.clone();
        let win_about = win_tray.clone();

        set_tray(
            tray()
                .icon(icons::tray_icon_src())
                .tooltip(tooltip)
                .on_click(move |_| {
                    let was_visible_focused = untracked(|| win_click.state())
                        .is_some_and(|st| st.visible && st.focused && !st.minimized);
                    toggle_window_visibility(&win_click);
                    if !was_visible_focused {
                        search_input_ref.focus();
                    }
                })
                .menu([
                    MenuItem::new("显示/隐藏窗口").on_select(move || {
                        let visible_unminimized = untracked(|| win_toggle.state())
                            .is_some_and(|st| st.visible && !st.minimized);
                        if visible_unminimized {
                            save_window_placement(
                                &win_toggle,
                                DEFAULT_WINDOW_PLACEMENT_KEY,
                                820.0,
                                540.0,
                            );
                            win_toggle.hide();
                        } else {
                            win_toggle.restore();
                            win_toggle.show();
                            win_toggle.focus();
                            search_input_ref.focus();
                        }
                    }),
                    MenuItem::new("设置与快捷键").on_select(move || {
                        win_settings.restore();
                        win_settings.show();
                        win_settings.focus();
                        store::set_settings_open(true);
                    }),
                    MenuItem::new("重新连接索引服务").on_select(store::reconnect_or_refresh),
                    MenuItem::new("关于与检查更新").on_select(move || {
                        win_about.restore();
                        win_about.show();
                        win_about.focus();
                        store::set_about_open(true);
                    }),
                    MenuItem::separator(),
                    autostart_tray_item(),
                    close_behavior_submenu(behavior, store::set_close_behavior),
                    MenuItem::separator(),
                    MenuItem::new("退出 Ramen").on_select(quit),
                ]),
        );
    });
    cleanup(|| set_tray(None));
}

fn title_bar() -> impl IntoElement {
    let left = title_bar_brand_clickable(crate::APP_META, || store::set_about_open(true));

    let badge_label = move || match store::daemon_state() {
        DaemonUiState::Connecting => "连接 gozd…".to_owned(),
        DaemonUiState::Connected {
            ready,
            version_matches,
            ..
        } => {
            if !version_matches {
                "版本不匹配".to_owned()
            } else if !ready || store::volumes_incomplete() {
                "索引构建中".to_owned()
            } else {
                let entries = store::total_indexed_entries();
                if entries > 0 {
                    format!("Live · {} 项", format_count(entries))
                } else {
                    "Live · 已就绪".to_owned()
                }
            }
        }
        DaemonUiState::Offline(svc) => format!("离线 · {}", svc.label()),
        DaemonUiState::Untrusted => "管道校验异常".to_owned(),
        DaemonUiState::Error(_) => "连接异常".to_owned(),
    };

    let badge_tone = move || match store::daemon_state() {
        DaemonUiState::Connecting => ValueTone::Info,
        DaemonUiState::Connected {
            ready,
            version_matches,
            ..
        } => {
            if !version_matches || !ready || store::volumes_incomplete() {
                ValueTone::Warn
            } else {
                ValueTone::Ok
            }
        }
        DaemonUiState::Offline(_) => ValueTone::Warn,
        DaemonUiState::Untrusted | DaemonUiState::Error(_) => ValueTone::Bad,
    };

    let center = view()
        .flex_1()
        .min_w(px(0.0))
        .flex_row()
        .items_center()
        .justify_center()
        .gap_2()
        .child(status_badge(badge_label, badge_tone));

    let actions = view()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .child(title_bar_autostart_toggle())
        .child(icon_action_spinning_with_tip(
            IconKind::Secondary,
            icons::REFRESH,
            "刷新或重新连接",
            "重新检测 gozd 服务并刷新结果",
            store::service_action_busy,
            || matches!(store::daemon_state(), DaemonUiState::Connecting) || store::service_action_busy(),
            store::reconnect_or_refresh,
        ))
        .child(icon_action_with_tip(
            IconKind::Secondary,
            icons::SETTINGS,
            "设置与快捷键",
            "设置、全局快捷键与 gozd 服务 (Ctrl+,)",
            || false,
            || store::set_settings_open(true),
        ))
        .child(about_icon_button(
            crate::APP_META,
            store::available_update_version,
            || store::set_about_open(true),
        ));

    let right = title_bar_right_with_theme(actions, store::theme_mode, store::set_theme_mode);

    title_bar_shell(48.0, left, center, right).bg(move || theme().sidebar)
}
