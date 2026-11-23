//! Main search workspace: prominent search bar, category filter chips, sortable
//! column headers, zero-jank `virtual_list`, onboarding fallback, and 28px status bar.

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::goz::{FileCategory, PINNED_GOZ_VERSION};
use crate::store::{self, DaemonUiState};
use crate::ui::{
    clickable_hotkey_badge, empty_state, filter_chip, format_bytes, format_count, icons,
    inline_notice_banner, result_row, results_table_header, scope_pill, search_toolbar_input,
    ValueTone,
};
use crate::views::onboarding;

fn category_label(cat: FileCategory) -> &'static str {
    match cat {
        FileCategory::All => "全部",
        FileCategory::Folders => "文件夹",
        FileCategory::Documents => "文档",
        FileCategory::Images => "图片",
        FileCategory::Videos => "视频",
        FileCategory::Audio => "音频",
        FileCategory::Archives => "压缩包",
        FileCategory::Code => "代码",
    }
}

/// Renders the primary search toolbar, filter strip, results or onboarding area, and status bar.
pub fn search_workspace(
    search_input_ref: ElementRef,
    results_list_ref: ElementRef,
    search_focused: Signal<bool>,
    search_has_selection: Signal<bool>,
) -> impl IntoElement {
    view()
        .flex_1()
        .min_h(px(0.0))
        .bg(move || theme().background)
        .child(search_header(
            search_input_ref,
            search_focused,
            search_has_selection,
        ))
        .child(version_mismatch_banner())
        .child(query_error_banner())
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .child(when(
                    move || !store::is_connected(),
                    onboarding::onboarding_view,
                ))
                .child(when(store::is_connected, move || {
                    results_surface(results_list_ref)
                })),
        )
        .child(status_bar())
}

fn search_header(
    search_input_ref: ElementRef,
    search_focused: Signal<bool>,
    search_has_selection: Signal<bool>,
) -> impl IntoElement {
    view()
        .px_4()
        .pt_3()
        .pb_2p5()
        .gap_2p5()
        .border_b_1()
        .border_color(move || theme().border)
        .bg(move || theme().background)
        .child(search_toolbar_input(
            search_input_ref,
            store::query,
            store::match_case,
            store::syntax_help_open,
            |text| store::set_query(text),
            store::clear_query,
            |_| {
                if let Some(row) = store::selected_or_first_row() {
                    store::open_entry(&row);
                }
            },
            store::toggle_match_case,
            || store::set_syntax_help_open(!store::syntax_help_open()),
            move || search_focused.set(true),
            move || {
                search_focused.set(false);
                search_has_selection.set(false);
            },
            move |sel: &Selection| search_has_selection.set(sel.start != sel.end),
        ))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    scroll()
                        .flex_1()
                        .min_w(px(0.0))
                        .horizontal()
                        .scrollbar(false)
                        .child(
                            view()
                                .test_id("category-filters")
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .children(FileCategory::ALL.into_iter().map(|cat| {
                                    filter_chip(
                                        move || store::category() == cat,
                                        category_label(cat),
                                        move || store::set_category(cat),
                                    )
                                })),
                        ),
                )
                .child(when(
                    move || store::scope().is_some(),
                    || {
                        scope_pill(
                            move || store::scope().unwrap_or_default(),
                            || store::set_scope(None),
                        )
                    },
                )),
        )
}

fn version_mismatch_banner() -> impl IntoElement {
    when(
        move || store::daemon_state().mismatched_server().is_some(),
        || {
            inline_notice_banner(
                ValueTone::Warn,
                icons::ALERT_TRIANGLE,
                move || {
                    let server = store::daemon_state()
                        .mismatched_server()
                        .unwrap_or("unknown")
                        .to_owned();
                    format!(
                        "当前连接的守护进程版本为 {server}，与 Ramen 目标版本 gozd {PINNED_GOZ_VERSION} 不一致，建议升级以获得完整特性支持。"
                    )
                },
                Some("升级到 goz v0.7.1"),
                store::service_action_busy,
                store::install_or_upgrade_service_action,
            )
        },
    )
}

fn query_error_banner() -> impl IntoElement {
    when(
        move || store::is_connected() && store::query_error().is_some(),
        || {
            inline_notice_banner(
                ValueTone::Bad,
                icons::ALERT_CIRCLE,
                move || store::query_error().unwrap_or_default(),
                Some("清空搜索"),
                || false,
                store::clear_query,
            )
        },
    )
}

fn results_surface(results_list_ref: ElementRef) -> impl IntoElement {
    view()
        .flex_1()
        .min_h(px(0.0))
        .child(results_table_header(store::sort, store::toggle_sort))
        .child(
            virtual_list(store::result_count, result_row)
                .keys(store::result_keys)
                .estimated_item_size(px(34.0))
                .separator(|| view().h(px(1.0)).bg(move || theme().border.opacity(0.45)))
                .empty(empty_state(
                    "没有匹配的文件或目录",
                    "尝试修改关键词、切换分类芯片，或点击右上角 ? 查看 goz 查询语法",
                ))
                .test_id("results-list")
                .flex_1()
                .element_ref(&results_list_ref),
        )
}

fn status_bar() -> impl IntoElement {
    let dot_color = move || match store::daemon_state() {
        DaemonUiState::Connecting => ValueTone::Info.color(),
        DaemonUiState::Connected {
            ready,
            version_matches,
            ..
        } => {
            if store::volumes_incomplete() || !ready || !version_matches {
                ValueTone::Warn.color()
            } else {
                ValueTone::Ok.color()
            }
        }
        DaemonUiState::Offline(_) => ValueTone::Warn.color(),
        DaemonUiState::Untrusted | DaemonUiState::Error(_) => ValueTone::Bad.color(),
    };

    let daemon_summary = move || match store::daemon_state() {
        DaemonUiState::Connecting => format!("goz v{PINNED_GOZ_VERSION} · 连接中"),
        DaemonUiState::Connected { ready, .. } => {
            let status_part = if let Some(status) = store::daemon_status() {
                let mounts: Vec<String> = status
                    .volumes
                    .iter()
                    .flat_map(|v| v.mounts.iter().cloned())
                    .collect();
                let total_entries = store::total_indexed_entries();
                if mounts.is_empty() {
                    format!("goz v{PINNED_GOZ_VERSION} · {} 项索引", format_count(total_entries))
                } else {
                    format!(
                        "goz v{PINNED_GOZ_VERSION} · {} · {} 项索引",
                        mounts.join(" "),
                        format_count(total_entries)
                    )
                }
            } else if ready {
                format!("goz v{PINNED_GOZ_VERSION} · 已连接")
            } else {
                format!("goz v{PINNED_GOZ_VERSION} · 初始化索引中")
            };
            status_part
        }
        DaemonUiState::Offline(svc) => format!("goz v{PINNED_GOZ_VERSION} · {}", svc.label()),
        DaemonUiState::Untrusted => format!("goz v{PINNED_GOZ_VERSION} · 管道校验异常"),
        DaemonUiState::Error(_) => format!("goz v{PINNED_GOZ_VERSION} · 连接异常"),
    };

    let match_summary = move || {
        if !store::is_connected() {
            return String::new();
        }
        let total = store::total_matches();
        let shown = store::result_count();
        if store::is_truncated() && (shown as u64) < total {
            format!(
                "显示前 {} / 共 {} 项",
                format_count(shown as u64),
                format_count(total)
            )
        } else {
            format!("{} 项", format_count(total))
        }
    };

    view()
        .h(px(28.0))
        .px_4()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .border_t_1()
        .border_color(move || theme().border)
        .bg(move || theme().sidebar)
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .flex_row()
                .items_center()
                .gap_2p5()
                .child(
                    view()
                        .flex_shrink_0()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(view().size_2().rounded_full().bg(dot_color))
                        .child(
                            text(daemon_summary)
                                .number_of_lines(1)
                                .text_xs()
                                .tabular_nums()
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(when(
                    move || store::is_connected() && store::volumes_incomplete(),
                    || {
                        text("索引构建中（结果可能不完整）")
                            .number_of_lines(1)
                            .text_xs()
                            .text_color(move || ValueTone::Warn.color())
                    },
                ))
                .child(when(
                    move || store::is_connected() && store::metadata_pending(),
                    || {
                        text("正在补充大小与时间…")
                            .number_of_lines(1)
                            .text_xs()
                            .text_color(move || ValueTone::Info.color())
                    },
                )),
        )
        .child(
            view()
                .flex_shrink_0()
                .flex_row()
                .items_center()
                .gap_3()
                .child(when(store::is_connected, move || {
                    view()
                        .flex_row()
                        .items_center()
                        .gap_2p5()
                        .child(
                            text(match_summary)
                                .text_xs()
                                .tabular_nums()
                                .text_color(move || theme().foreground),
                        )
                        .child(when(
                            move || store::total_size().is_some(),
                            || {
                                text(move || {
                                    store::total_size()
                                        .map(format_bytes)
                                        .unwrap_or_default()
                                })
                                .text_xs()
                                .tabular_nums()
                                .text_color(move || theme().muted_foreground)
                            },
                        ))
                        .child(when(
                            move || store::elapsed_ms().is_some(),
                            || {
                                text(move || {
                                    store::elapsed_ms()
                                        .map(|ms| format!("{ms} ms"))
                                        .unwrap_or_default()
                                })
                                .text_xs()
                                .tabular_nums()
                                .text_color(move || theme().muted_foreground)
                            },
                        ))
                }))
                .child(clickable_hotkey_badge(
                    || {
                        let binding = store::global_hotkey();
                        if binding.enabled {
                            binding.display_label()
                        } else {
                            "热键未启用".to_owned()
                        }
                    },
                    || store::set_settings_open(true),
                )),
        )
}
