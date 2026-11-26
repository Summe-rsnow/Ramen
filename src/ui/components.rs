//! Ramen-specific styled UI primitives built on `nsh_base` and `bento::ui`
//! using 100% semantic `theme()` tokens and zero per-row signals.

use nsh_base::{button as base_button, input, theme};
use nsh_core::element::desktop::{popup_menu_at, MenuItem};
use nsh_core::prelude::*;

use super::icons::{self, svg_icon};
use super::{
    action_sm_disabled, focus_ring, hint_tooltip, hotkey_badge, icon_action, mono_font, radius,
    ButtonKind, IconKind, ValueTone, MOTION_FAST_MS,
};
use crate::goz::{FileIconKind, SortDir, SortKey, SortSpec};
use crate::store::{self, SearchRow};

const SIZE_COL_WIDTH: f32 = 92.0;
const MTIME_COL_WIDTH: f32 = 136.0;
const ACTIONS_COL_WIDTH: f32 = 88.0;

/// Prominent 40pt search bar with leading search icon, clear button, `Aa` match-case toggle,
/// and `?` syntax-help toggle.
#[allow(clippy::too_many_arguments)]
pub fn search_toolbar_input(
    input_ref: ElementRef,
    value: impl Fn() -> String + Copy + 'static,
    match_case: impl Fn() -> bool + Copy + 'static,
    syntax_open: impl Fn() -> bool + Copy + 'static,
    on_change: impl Fn(&str) + 'static,
    on_clear: impl Fn() + 'static,
    on_submit: impl Fn(&str) + 'static,
    on_toggle_case: impl Fn() + 'static,
    on_toggle_syntax: impl Fn() + 'static,
    on_focus: impl Fn() + 'static,
    on_blur: impl Fn() + 'static,
    on_selection_change: impl Fn(&Selection) + 'static,
) -> View {
    let on_clear = std::rc::Rc::new(on_clear);
    let reduced = reduce_motion();

    let case_btn = base_button()
        .on_press(on_toggle_case)
        .selected(match_case)
        .accessibility_label("区分大小写")
        .h(px(28.0))
        .px_2()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(move || {
            if match_case() {
                theme().primary
            } else {
                theme().border
            }
        })
        .bg(move || {
            if match_case() {
                theme().accent
            } else {
                theme().background
            }
        })
        .hover(move |s| {
            s.bg(move || {
                if match_case() {
                    theme().accent
                } else {
                    theme().muted
                }
            })
        })
        .when(!reduced, |b| {
            b.active(|s| s.scale(0.95))
                .transition_transform()
                .duration(MOTION_FAST_MS)
        })
        .focus_visible(focus_ring)
        .child(
            text("Aa")
                .text_xs()
                .font_family(mono_font)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || {
                    if match_case() {
                        theme().accent_foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
        );

    let help_btn = base_button()
        .on_press(on_toggle_syntax)
        .selected(syntax_open)
        .accessibility_label("搜索语法")
        .w(px(28.0))
        .h(px(28.0))
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(move || {
            if syntax_open() {
                theme().primary
            } else {
                theme().border
            }
        })
        .bg(move || {
            if syntax_open() {
                theme().accent
            } else {
                theme().background
            }
        })
        .hover(move |s| {
            s.bg(move || {
                if syntax_open() {
                    theme().accent
                } else {
                    theme().muted
                }
            })
        })
        .when(!reduced, |b| {
            b.active(|s| s.scale(0.95))
                .transition_transform()
                .duration(MOTION_FAST_MS)
        })
        .focus_visible(focus_ring)
        .child(
            text("?")
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || {
                    if syntax_open() {
                        theme().accent_foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
        );

    view()
        .h(px(40.0))
        .pl_3()
        .pr(px(6.0))
        .flex_row()
        .items_center()
        .gap_2()
        .rounded(radius)
        .border_1()
        .border_color(move || theme().input)
        .bg(move || theme().card)
        .focus_within(|s| s.border_color(move || theme().ring))
        .child(svg_icon(icons::SEARCH, 15.0, move || {
            theme().muted_foreground
        }))
        .child(
            input()
                .element_ref(&input_ref)
                .value(value)
                .on_change(on_change)
                .on_submit(on_submit)
                .on_focus(on_focus)
                .on_blur(on_blur)
                .on_selection_change(on_selection_change)
                .submit_behavior(SubmitBehavior::Submit)
                .auto_focus(true)
                .select_text_on_focus(true)
                .placeholder("搜索文件、文件夹或 goz 语法 (如 *.rs, ext:pdf, size:>10mb)…")
                .placeholder_color(|| color(theme().muted_foreground))
                .caret_color(|| color(theme().primary))
                .label("搜索全盘文件")
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .text_sm()
                .text_color(move || theme().foreground),
        )
        .child(when(move || !value().is_empty(), move || {
            let on_clear = std::rc::Rc::clone(&on_clear);
            icon_action(IconKind::Subtle, icons::CLOSE, "清空搜索", move || {
                on_clear()
            })
        }))
        .child(
            view()
                .flex_shrink_0()
                .tooltip(hint_tooltip("区分大小写 (Match Case)"))
                .child(case_btn),
        )
        .child(
            view()
                .flex_shrink_0()
                .tooltip(hint_tooltip("搜索语法帮助"))
                .child(help_btn),
        )
}

/// Active directory scope pill with a one-click `×` clear button.
pub fn scope_pill(
    scope_text: impl Fn() -> String + Copy + 'static,
    on_clear: impl Fn() + 'static,
) -> impl IntoElement {
    let reduced = reduce_motion();
    view()
        .flex_shrink_0()
        .h(px(26.0))
        .pl_2()
        .pr(px(4.0))
        .max_w(px(260.0))
        .flex_row()
        .items_center()
        .gap_1()
        .rounded_full()
        .border_1()
        .border_color(move || theme().primary)
        .bg(move || theme().accent)
        .child(svg_icon(icons::FOLDER_OPEN, 12.0, move || {
            theme().accent_foreground
        }))
        .child(
            text(move || format!("目录: {}", scope_text()))
                .min_w(px(0.0))
                .flex_shrink(1.0)
                .number_of_lines(1)
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().accent_foreground),
        )
        .child(
            base_button()
                .on_press(on_clear)
                .accessibility_label("清除目录范围")
                .w(px(18.0))
                .h(px(18.0))
                .items_center()
                .justify_center()
                .rounded_full()
                .hover(|s| s.bg(move || theme().background.opacity(0.6)))
                .when(!reduced, |b| b.active(|s| s.scale(0.92)))
                .focus_visible(focus_ring)
                .child(svg_icon(icons::CLOSE, 11.0, move || {
                    theme().accent_foreground
                })),
        )
}

/// Clickable sortable column header row aligned to the exact 4-column spine of [`result_row`].
pub fn results_table_header(
    current_sort: impl Fn() -> SortSpec + Copy + 'static,
    on_sort: impl Fn(SortKey) + Copy + 'static,
) -> impl IntoElement {
    view()
        .h(px(30.0))
        .px_4()
        .flex_row()
        .items_center()
        .gap_3()
        .border_b_1()
        .border_color(move || theme().border)
        .bg(move || theme().card)
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    view()
                        .flex_1()
                        .flex_grow(2.0)
                        .min_w(px(0.0))
                        .child(sort_header_cell(
                            "名称",
                            "按名称排序",
                            SortKey::Name,
                            false,
                            current_sort,
                            move || on_sort(SortKey::Name),
                        )),
                )
                .child(
                    view()
                        .flex_1()
                        .flex_grow(3.0)
                        .min_w(px(0.0))
                        .child(sort_header_cell(
                            "路径",
                            "按路径排序",
                            SortKey::Path,
                            false,
                            current_sort,
                            move || on_sort(SortKey::Path),
                        )),
                )
                .child(
                    view()
                        .w(px(SIZE_COL_WIDTH))
                        .flex_shrink_0()
                        .items_end()
                        .child(sort_header_cell(
                            "大小",
                            "按大小排序",
                            SortKey::Size,
                            true,
                            current_sort,
                            move || on_sort(SortKey::Size),
                        )),
                )
                .child(
                    view()
                        .w(px(MTIME_COL_WIDTH))
                        .flex_shrink_0()
                        .items_end()
                        .child(sort_header_cell(
                            "修改时间",
                            "按修改时间排序",
                            SortKey::DateModified,
                            true,
                            current_sort,
                            move || on_sort(SortKey::DateModified),
                        )),
                ),
        )
        .child(view().w(px(ACTIONS_COL_WIDTH)).flex_shrink_0())
}

fn sort_header_cell(
    label: &'static str,
    a11y_label: &'static str,
    key: SortKey,
    align_right: bool,
    current_sort: impl Fn() -> SortSpec + Copy + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    let is_active = move || current_sort().key == key;
    let btn = base_button()
        .on_press(on_press)
        .selected(is_active)
        .accessibility_label(a11y_label)
        .h(px(24.0))
        .px(px(6.0))
        .flex_row()
        .items_center()
        .gap_1()
        .rounded(px(6.0))
        .hover(|s| s.bg(move || theme().muted))
        .active(|s| s.bg(move || theme().accent))
        .focus_visible(focus_ring)
        .child(
            text(label)
                .text_xs()
                .style_with(move |s| {
                    if is_active() {
                        s.font_weight(FontWeight::SEMIBOLD)
                            .text_color(move || theme().foreground)
                    } else {
                        s.font_weight(FontWeight::MEDIUM)
                            .text_color(move || theme().muted_foreground)
                    }
                }),
        )
        .child(when(is_active, move || {
            let icon = if current_sort().dir == SortDir::Asc {
                icons::ARROW_UP
            } else {
                icons::ARROW_DOWN
            };
            svg_icon(icon, 11.0, move || theme().primary)
        }));

    if align_right {
        view().w_full().items_end().child(btn).into_element()
    } else {
        view().w_full().items_start().child(btn).into_element()
    }
}

/// Fixed 34pt virtualized result row with zero per-row local signals.
///
/// Supports click selection, double-click open, right-click `popup_menu`, and accessible
/// inline quick action buttons (`打开` / `定位文件夹` / `复制路径`).
pub fn result_row(at: Position) -> impl IntoElement {
    let row = computed(move || store::result_at(at.index()));
    let is_selected = move || store::selected_index() == Some(at.index());

    view()
        .w_full()
        .h(px(34.0))
        .px_4()
        .flex_row()
        .items_center()
        .gap_3()
        .bg(move || {
            if is_selected() {
                theme().accent
            } else {
                theme().background
            }
        })
        .hover(move |s| {
            s.bg(move || {
                if is_selected() {
                    theme().accent
                } else {
                    theme().muted
                }
            })
        })
        .on_context_menu(move |e: &ContextMenuEvent| {
            let idx = untracked(|| at.index());
            let Some(r) = store::result_at_untracked(idx) else {
                return;
            };
            store::set_selected_index(Some(idx));
            show_row_context_menu(&r, e.page_position);
        })
        .child(
            view()
                .role(AccessibilityRole::Button)
                .focusable(true)
                .accessibility_state(move || AccessibilityState {
                    selected: is_selected(),
                    ..AccessibilityState::default()
                })
                .optional_label(move || {
                    Some(SharedString::from(row.with(|r| {
                        r.as_ref().map(|x| x.item.path.clone()).unwrap_or_default()
                    })))
                })
                .on_press(move |e: &PressEvent| {
                    let idx = untracked(|| at.index());
                    store::set_selected_index(Some(idx));
                    if e.clicks >= 2 {
                        if let Some(r) = row.get_untracked() {
                            store::open_entry(&r);
                        }
                    }
                })
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .flex_row()
                .items_center()
                .gap_3()
                .rounded(px(4.0))
                .focus_visible(focus_ring)
                .child(
                    // Lane 1: File icon + file name
                    view()
                        .flex_1()
                        .flex_grow(2.0)
                        .min_w(px(0.0))
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            view()
                                .w(px(20.0))
                                .flex_shrink_0()
                                .items_center()
                                .justify_center()
                                .child(
                                    svg(move || {
                                        let kind = row.with(|r| {
                                            r.as_ref()
                                                .map_or(FileIconKind::Other, |x| x.icon_kind)
                                        });
                                        nsh_core::element::ImageSrc::svg_markup(
                                            icons::file_icon_markup(kind),
                                        )
                                    })
                                    .size(px(15.0))
                                    .text_color(move || {
                                        let kind = row.with(|r| {
                                            r.as_ref()
                                                .map_or(FileIconKind::Other, |x| x.icon_kind)
                                        });
                                        icons::file_icon_color(kind, is_selected())
                                    }),
                                ),
                        )
                        .child(
                            text(move || {
                                row.with(|r| {
                                    r.as_ref().map(|x| x.file_name.clone()).unwrap_or_default()
                                })
                            })
                            .flex_1()
                            .min_w(px(0.0))
                            .number_of_lines(1)
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(move || {
                                if is_selected() {
                                    theme().accent_foreground
                                } else {
                                    theme().foreground
                                }
                            }),
                        ),
                )
                .child(
                    // Lane 2: Parent directory path
                    view().flex_1().flex_grow(3.0).min_w(px(0.0)).child(
                        text(move || {
                            row.with(|r| {
                                r.as_ref().map(|x| x.parent_dir.clone()).unwrap_or_default()
                            })
                        })
                        .w_full()
                        .number_of_lines(1)
                        .text_xs()
                        .text_color(move || {
                            if is_selected() {
                                theme().accent_foreground
                            } else {
                                theme().muted_foreground
                            }
                        }),
                    ),
                )
                .child(
                    // Lane 3: Size (tabular numerals, right-aligned)
                    view().w(px(SIZE_COL_WIDTH)).flex_shrink_0().items_end().child(
                        text(move || {
                            row.with(|r| {
                                r.as_ref().map(|x| x.size_label.clone()).unwrap_or_default()
                            })
                        })
                        .number_of_lines(1)
                        .text_xs()
                        .tabular_nums()
                        .text_color(move || {
                            if is_selected() {
                                theme().accent_foreground
                            } else {
                                theme().muted_foreground
                            }
                        }),
                    ),
                )
                .child(
                    // Lane 4: Date Modified (tabular numerals, right-aligned)
                    view().w(px(MTIME_COL_WIDTH)).flex_shrink_0().items_end().child(
                        text(move || {
                            row.with(|r| {
                                r.as_ref()
                                    .map(|x| x.mtime_label.clone())
                                    .unwrap_or_default()
                            })
                        })
                        .number_of_lines(1)
                        .text_xs()
                        .tabular_nums()
                        .text_color(move || {
                            if is_selected() {
                                theme().accent_foreground
                            } else {
                                theme().muted_foreground
                            }
                        }),
                    ),
                ),
        )
        .child(
            // Lane 5: Quick row actions (always keyboard & pointer reachable)
            view()
                .w(px(ACTIONS_COL_WIDTH))
                .flex_shrink_0()
                .flex_row()
                .items_center()
                .justify_end()
                .gap_1()
                .child(icon_action(IconKind::Subtle, icons::EXTERNAL_LINK, "打开", move || {
                    if let Some(r) = row.get_untracked() {
                        store::open_entry(&r);
                    }
                }))
                .child(icon_action(
                    IconKind::Subtle,
                    icons::FOLDER_OPEN,
                    "定位文件夹",
                    move || {
                        if let Some(r) = row.get_untracked() {
                            store::reveal_entry(&r);
                        }
                    },
                ))
                .child(icon_action(
                    IconKind::Subtle,
                    icons::COPY,
                    "复制路径",
                    move || {
                        if let Some(r) = row.get_untracked() {
                            store::copy_path(&r.item.path, "路径");
                        }
                    },
                )),
        )
}

fn show_row_context_menu(row: &SearchRow, at: Point<Points>) {
    let row_open = row.clone();
    let row_reveal = row.clone();
    let full_path = row.item.path.clone();
    let file_name = row.file_name.to_string();
    let parent_dir = if row.parent_dir.is_empty() {
        row.item.path.clone()
    } else {
        row.parent_dir.to_string()
    };
    let scope_dir = if row.item.is_dir {
        row.item.path.clone()
    } else if !row.parent_dir.is_empty() {
        row.parent_dir.to_string()
    } else {
        row.item.path.clone()
    };

    let _ = popup_menu_at(
        vec![
            MenuItem::new("打开").on_select(move || store::open_entry(&row_open)),
            MenuItem::new("在资源管理器中显示")
                .on_select(move || store::reveal_entry(&row_reveal)),
            MenuItem::separator(),
            MenuItem::new("复制完整路径")
                .on_select(move || store::copy_path(&full_path, "路径")),
            MenuItem::new("复制文件名")
                .on_select(move || store::copy_path(&file_name, "文件名")),
            MenuItem::new("复制所在目录")
                .on_select(move || store::copy_path(&parent_dir, "目录")),
            MenuItem::separator(),
            MenuItem::new("仅在此目录搜索")
                .on_select(move || store::set_scope(Some(scope_dir.clone()))),
        ],
        at,
    );
}

/// Clickable global-hotkey pill shown in the bottom status bar.
pub fn clickable_hotkey_badge(
    label: impl Fn() -> String + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    let reduced = reduce_motion();
    view().tooltip(hint_tooltip("点击设置全局召唤快捷键")).child(
        base_button()
            .on_press(on_press)
            .accessibility_label("配置全局快捷键")
            .rounded_full()
            .hover(|s| s.opacity(0.85))
            .when(!reduced, |b| b.active(|s| s.scale(0.97)))
            .focus_visible(focus_ring)
            .child(hotkey_badge(label)),
    )
}

/// Inline semantic alert banner (warning or error) with optional trailing recovery action.
pub fn inline_notice_banner(
    tone: ValueTone,
    icon_markup: &'static str,
    message: impl Fn() -> String + 'static,
    action_label: Option<&'static str>,
    action_disabled: impl Fn() -> bool + 'static,
    on_action: impl Fn() + 'static,
) -> impl IntoElement {
    let mut row = view()
        .w_full()
        .px_4()
        .py_2()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .border_b_1()
        .border_color(move || theme().border)
        .bg(move || theme().muted)
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .flex_row()
                .items_center()
                .gap_2()
                .child(svg_icon(icon_markup, 14.0, move || tone.color()))
                .child(
                    text(message)
                        .flex_1()
                        .min_w(px(0.0))
                        .number_of_lines(2)
                        .text_xs()
                        .text_color(move || theme().foreground),
                ),
        );

    if let Some(btn_label) = action_label {
        row = row.child(action_sm_disabled(
            ButtonKind::Secondary,
            btn_label,
            action_disabled,
            on_action,
        ));
    }

    row
}

