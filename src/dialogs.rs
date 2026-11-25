//! Modal dialogs for Ramen:
//! - `settings_dialog_overlay`: Global hotkey editor, close behavior, auto-refresh & `gozd v0.7.1` service management
//! - `syntax_help_overlay`: Interactive `goz` search syntax reference (`*.rs`, `folder:`, `ext:`, `size:>10mb`, `dm:today`, `regex:`, `!exclude`, `C:\path\`)
//! - `confirm_uninstall_overlay`: Destructive UAC confirmation before removing `goz` Windows service
//! - `about_dialog_overlay`: Shared `bento::updater::about_dialog`

use bento::shell::CloseBehavior;
use nsh_base::theme;
use nsh_core::prelude::*;

use crate::goz::{DaemonServiceState, VolumePhase, PINNED_GOZ_VERSION};
use crate::store::{self, DaemonUiState};
use crate::ui::{
    action, action_disabled, action_sm, card, confirm_dialog, divider, format_bytes, format_count,
    hint_tooltip, hotkey_editor, icons, meta_row, modal_dialog, radio_option, radius,
    section_header, status_badge, toggle_switch, ButtonKind, ValueTone,
};

/// Shared Bento About & GitHub Releases update checker dialog.
pub fn about_dialog_overlay() -> impl IntoElement {
    when(store::about_open, || {
        bento::updater::about_dialog(
            crate::APP_META,
            store::update_status,
            store::check_for_updates,
            || store::set_about_open(false),
        )
    })
}

/// Settings dialog hosting global hotkey configuration, window close behavior, and `gozd v0.7.1` management.
pub fn settings_dialog_overlay() -> impl IntoElement {
    when(store::settings_open, settings_dialog_content)
}

fn settings_dialog_content() -> impl IntoElement {
    let close = || store::set_settings_open(false);

    modal_dialog(
        520.0,
        close,
        view()
            .gap_4()
            .child(
                view()
                    .gap_1()
                    .child(
                        text("设置与服务")
                            .role(AccessibilityRole::Header)
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(move || theme().foreground),
                    )
                    .child(
                        text(format!(
                            "配置全局召唤快捷键、窗口托盘行为与 gozd v{PINNED_GOZ_VERSION} 全盘索引服务"
                        ))
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                    ),
            )
            .child(
                scroll().max_h(px(380.0)).child(
                    view()
                        .gap_3()
                        .pr_1()
                        .child(settings_hotkey_section())
                        .child(settings_window_section())
                        .child(settings_service_section()),
                ),
            )
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(toggle_switch(
                        store::autostart_enabled,
                        "开机自启",
                        Some("开机自启"),
                        store::set_autostart_enabled,
                    ))
                    .child(action(ButtonKind::Secondary, "关闭", close)),
            ),
    )
}

fn settings_hotkey_section() -> impl IntoElement {
    card()
        .p_4()
        .gap_3()
        .child(section_title(icons::KEYBOARD, "全局召唤快捷键"))
        .child(hotkey_editor(
            store::global_hotkey,
            store::hotkey_status,
            store::set_global_hotkey,
        ))
}

fn settings_window_section() -> impl IntoElement {
    card()
        .p_4()
        .gap_3()
        .child(section_title(icons::SETTINGS, "窗口与刷新行为"))
        .child(
            view()
                .gap_2()
                .child(radio_option(
                    move || store::close_behavior() == CloseBehavior::Ask,
                    CloseBehavior::Ask.label(),
                    "点击关闭按钮时弹出选择框，决定最小化到托盘或退出",
                    "每次询问",
                    || store::set_close_behavior(CloseBehavior::Ask),
                ))
                .child(radio_option(
                    move || store::close_behavior() == CloseBehavior::Tray,
                    CloseBehavior::Tray.label(),
                    "隐藏主窗口并驻留托盘，可随时用全局快捷键唤醒",
                    "最小化到托盘",
                    || store::set_close_behavior(CloseBehavior::Tray),
                ))
                .child(radio_option(
                    move || store::close_behavior() == CloseBehavior::Exit,
                    CloseBehavior::Exit.label(),
                    "直接退出 Ramen 窗口并关闭托盘图标",
                    "直接退出应用",
                    || store::set_close_behavior(CloseBehavior::Exit),
                )),
        )
        .child(divider())
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(2.0))
                        .child(
                            text("文件变化时自动刷新搜索结果")
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            text("通过 goz Watch 事件在磁盘文件增删改后自动重查当前关键词")
                                .text_xs()
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(toggle_switch(
                    store::auto_refresh,
                    "文件变化时自动刷新",
                    None,
                    store::set_auto_refresh,
                )),
        )
}

fn settings_service_section() -> impl IntoElement {
    let state_label = move || match store::daemon_state() {
        DaemonUiState::Connecting => "正在连接…".to_owned(),
        DaemonUiState::Connected {
            server,
            version_matches: true,
            ..
        } => format!("已连接 ({server})"),
        DaemonUiState::Connected {
            server,
            version_matches: false,
            ..
        } => format!("版本不一致 ({server})"),
        DaemonUiState::Offline(svc) => format!("未连接 · {}", svc.label()),
        DaemonUiState::Untrusted => "管道校验未通过".to_owned(),
        DaemonUiState::Error(_) => "连接异常".to_owned(),
    };

    let state_tone = move || match store::daemon_state() {
        DaemonUiState::Connecting => ValueTone::Info,
        DaemonUiState::Connected {
            version_matches: true,
            ..
        } => ValueTone::Ok,
        DaemonUiState::Connected {
            version_matches: false,
            ..
        }
        | DaemonUiState::Offline(_) => ValueTone::Warn,
        DaemonUiState::Untrusted | DaemonUiState::Error(_) => ValueTone::Bad,
    };

    let can_start_existing = move || {
        matches!(
            store::daemon_state(),
            DaemonUiState::Offline(
                DaemonServiceState::Stopped
                    | DaemonServiceState::Disabled
                    | DaemonServiceState::Paused
            )
        )
    };

    let can_uninstall = move || {
        !matches!(
            store::daemon_state(),
            DaemonUiState::Offline(
                DaemonServiceState::NotInstalled | DaemonServiceState::UnsupportedPlatform
            )
        )
    };

    card()
        .p_4()
        .gap_3()
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(section_title(
                    icons::HARD_DRIVE,
                    "后台索引服务 (gozd v0.7.1)",
                ))
                .child(status_badge(state_label, state_tone)),
        )
        .child(
            view()
                .p_3()
                .gap_2()
                .rounded(radius)
                .border_1()
                .border_color(move || theme().border)
                .bg(move || theme().muted)
                .child(meta_row(
                    "目标服务版本",
                    text(format!("gozd v{PINNED_GOZ_VERSION}"))
                        .text_xs()
                        .tabular_nums()
                        .text_color(move || theme().foreground),
                ))
                .child(when(
                    move || store::daemon_status().is_some(),
                    || {
                        let mem_text = move || {
                            store::daemon_status()
                                .map(|st| {
                                    format!(
                                        "工作集 {} · 私有提交 {}",
                                        format_bytes(st.process_working_set),
                                        format_bytes(st.process_private_bytes)
                                    )
                                })
                                .unwrap_or_default()
                        };
                        meta_row(
                            "进程内存占用",
                            text(mem_text)
                                .text_xs()
                                .tabular_nums()
                                .text_color(move || theme().foreground),
                        )
                    },
                )),
        )
        .child(when(
            move || {
                store::daemon_status()
                    .is_some_and(|st| !st.volumes.is_empty())
            },
            volume_list_view,
        ))
        .child(when(store::service_action_busy, || {
            text(move || {
                store::service_action_label()
                    .unwrap_or_else(|| "正在执行服务管理命令…".to_owned())
            })
            .text_xs()
            .text_color(move || theme().primary)
        }))
        .child(when(
            move || store::service_action_error().is_some(),
            || {
                text(move || store::service_action_error().unwrap_or_default())
                    .text_xs()
                    .text_color(move || theme().destructive)
            },
        ))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(action_disabled(
                    ButtonKind::Primary,
                    format!("安装 / 升级 goz v{PINNED_GOZ_VERSION}"),
                    move || {
                        store::service_action_busy()
                            || matches!(
                                store::daemon_state(),
                                DaemonUiState::Offline(DaemonServiceState::UnsupportedPlatform)
                            )
                    },
                    store::install_or_upgrade_service_action,
                ))
                .child(when(can_start_existing, || {
                    action_disabled(
                        ButtonKind::Secondary,
                        "启动服务",
                        store::service_action_busy,
                        store::start_service_action,
                    )
                }))
                .child(action_disabled(
                    ButtonKind::Secondary,
                    "刷新状态",
                    store::service_action_busy,
                    store::reconnect_or_refresh,
                ))
                .child(when(can_uninstall, || {
                    action_disabled(
                        ButtonKind::Destructive,
                        "卸载服务…",
                        store::service_action_busy,
                        || store::set_uninstall_confirm_open(true),
                    )
                })),
        )
}

fn volume_list_view() -> impl IntoElement {
    let volumes = computed(|| {
        store::daemon_status()
            .map(|st| st.volumes)
            .unwrap_or_default()
    });

    view().gap_1p5().child(
        dynamic(move || {
            let list = volumes.get();
            untracked(|| {
                view().gap_1p5().children(list.into_iter().map(|vol| {
                    let mounts = if vol.mounts.is_empty() {
                        vol.guid.clone()
                    } else {
                        vol.mounts.join(", ")
                    };
                    let phase_tone = match &vol.phase {
                        VolumePhase::Live | VolumePhase::Snapshot => ValueTone::Ok,
                        VolumePhase::Bootstrapping | VolumePhase::Rescanning => ValueTone::Warn,
                        VolumePhase::Offline | VolumePhase::Failed { .. } => ValueTone::Bad,
                    };
                    let phase_label = match &vol.phase {
                        VolumePhase::Live => "Live".to_owned(),
                        VolumePhase::Snapshot => "Snapshot".to_owned(),
                        VolumePhase::Bootstrapping => "Bootstrapping".to_owned(),
                        VolumePhase::Rescanning => "Rescanning".to_owned(),
                        VolumePhase::Offline => "Offline".to_owned(),
                        VolumePhase::Failed { .. } => "Failed".to_owned(),
                    };
                    let mem_bytes = vol.memory.as_ref().map(|m| {
                        m.entries.used
                            + m.arena_raw.used
                            + m.arena_folded.used
                            + m.frn_map.used
                            + m.name_tables.used
                            + m.dir_children.used
                    });
                    let detail = match mem_bytes {
                        Some(bytes) if bytes > 0 => format!(
                            "{} 项 · {}",
                            format_count(vol.entries),
                            format_bytes(bytes)
                        ),
                        _ => format!("{} 项", format_count(vol.entries)),
                    };

                    let full_path = mounts.clone();
                    view()
                        .px_3()
                        .py_2()
                        .rounded(radius)
                        .border_1()
                        .border_color(move || theme().border)
                        .bg(move || theme().background)
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            view()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(
                                    view()
                                        .min_w(px(0.0))
                                        .flex_shrink(1.0)
                                        .max_w(px(200.0))
                                        .overflow_hidden()
                                        .tooltip(hint_tooltip(full_path))
                                        .child(
                                            text(mounts)
                                                .min_w(px(0.0))
                                                .flex_shrink(1.0)
                                                .truncate()
                                                .text_xs()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(move || theme().foreground),
                                        ),
                                )
                                .child(status_badge(phase_label, phase_tone)),
                        )
                        .child(
                            text(detail)
                                .flex_shrink_0()
                                .number_of_lines(1)
                                .text_xs()
                                .tabular_nums()
                                .text_color(move || theme().muted_foreground),
                        )
                }))
            })
        }),
    )
}

/// Destructive confirmation dialog before uninstalling the `goz` Windows service.
pub fn confirm_uninstall_overlay() -> impl IntoElement {
    when(store::uninstall_confirm_open, || {
        confirm_dialog(
            420.0,
            true,
            "确认卸载 goz 索引服务",
            "卸载将停止并移除 Windows 系统服务 \"goz\" 及其安装目录中的 Program Files\\goz\\gozd.exe。卸载后将无法继续全盘毫秒搜索，需要时可随时重新安装。",
            None,
            "取消",
            "确认卸载",
            ButtonKind::Destructive,
            || store::set_uninstall_confirm_open(false),
            || {
                store::set_uninstall_confirm_open(false);
                store::uninstall_service_action();
            },
        )
    })
}

/// Search syntax quick-reference modal dialog with one-click example insertion.
pub fn syntax_help_overlay() -> impl IntoElement {
    when(store::syntax_help_open, || {
        let close = || store::set_syntax_help_open(false);
        let examples: [(&'static str, &'static str); 8] = [
            ("*.rs", "通配符匹配文件扩展名（如所有 Rust 源码）"),
            ("folder:", "仅搜索文件夹目录（等同点击「文件夹」筛选芯片）"),
            ("ext:pdf;docx", "同时匹配多个指定扩展名，分号分隔"),
            ("size:>10mb", "按文件体积筛选（支持 kb / mb / gb 与 > < = 范围）"),
            ("dm:today", "按修改日期筛选（today / yesterday / thisweek / 2026）"),
            ("regex:", "启用正则表达式匹配（如 regex:^cargo\\.(toml|lock)$）"),
            ("!exclude", "在关键词前加 ! 排除匹配项（如 report !draft）"),
            (r"C:\path\", "在搜索框中输入路径前缀限定或过滤目录"),
        ];

        modal_dialog(
            500.0,
            close,
            view()
                .gap_4()
                .child(
                    view()
                        .gap_1()
                        .child(
                            text("goz 搜索语法速查")
                                .role(AccessibilityRole::Header)
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            text("支持空格逻辑与、引号精确短语以及 Everything 兼容过滤前缀；点击示例即可填入搜索框")
                                .text_xs()
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(
                    view()
                        .gap_2()
                        .children(examples.into_iter().map(|(syntax, desc)| {
                            let sample = syntax.to_owned();
                            view()
                                .px_3()
                                .py_2()
                                .rounded(radius)
                                .border_1()
                                .border_color(move || theme().border)
                                .bg(move || theme().muted)
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    text(desc)
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .text_xs()
                                        .text_color(move || theme().foreground),
                                )
                                .child(action_sm(ButtonKind::Secondary, syntax, move || {
                                    store::set_query(sample.clone());
                                    store::set_syntax_help_open(false);
                                }))
                        })),
                )
                .child(
                    view()
                        .flex_row()
                        .justify_end()
                        .child(action(ButtonKind::Primary, "关闭", close)),
                ),
        )
    })
}

fn section_title(icon_markup: &'static str, title: &'static str) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap_2()
        .child(icons::svg_icon(icon_markup, 15.0, move || theme().primary))
        .child(section_header(title))
}
