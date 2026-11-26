//! Service onboarding and recovery view shown when `gozd` v0.7.1 is not yet connected.

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::goz::{DaemonServiceState, PINNED_GOZ_VERSION};
use crate::store::{self, DaemonUiState};
use crate::ui::{
    action, action_busy, action_disabled, card, empty_state, icons, status_badge, ButtonKind,
    ValueTone,
};

/// Renders either a brief connecting view or an actionable service onboarding/recovery card.
pub fn onboarding_view() -> impl IntoElement {
    let kind_key = computed(|| match store::daemon_state() {
        DaemonUiState::Connecting => 0_u8,
        DaemonUiState::Offline(svc) => match svc {
            DaemonServiceState::NotInstalled => 1,
            DaemonServiceState::Stopped
            | DaemonServiceState::Disabled
            | DaemonServiceState::Paused => 2,
            DaemonServiceState::Starting
            | DaemonServiceState::Stopping
            | DaemonServiceState::Running => 3,
            DaemonServiceState::UnsupportedPlatform => 4,
        },
        DaemonUiState::Untrusted => 5,
        DaemonUiState::Error(_) => 6,
        DaemonUiState::Connected { .. } => 7,
    });

    view().flex_1().child(dynamic(move || {
        let _ = kind_key.get();
        let state = untracked(store::daemon_state_untracked);
        untracked(|| match state {
            DaemonUiState::Connecting => empty_state(
                format!("正在连接 goz v{PINNED_GOZ_VERSION} 索引服务…"),
                "正在通过 Named Pipe 与后台守护进程握手，请稍候",
            )
            .into_element(),
            DaemonUiState::Offline(svc) => offline_card(svc).into_element(),
            DaemonUiState::Untrusted => untrusted_card().into_element(),
            DaemonUiState::Error(msg) => error_card(msg).into_element(),
            DaemonUiState::Connected { .. } => view().into_element(),
        })
    }))
}

fn offline_card(svc: DaemonServiceState) -> impl IntoElement {
    let (title, body, badge_tone, icon_markup) = match svc {
        DaemonServiceState::NotInstalled => (
            "安装并启用全盘高速索引服务",
            format!(
                "Ramen 已随包捆绑 goz v{PINNED_GOZ_VERSION} 后台索引服务（gozd.exe），通过读取 NTFS MFT 与 USN 日志实现毫秒级全盘文件极速搜索。点击下方按钮可通过管理员权限安装至 Program Files\\goz\\gozd.exe 并启动服务。"
            ),
            ValueTone::Info,
            icons::ROCKET,
        ),
        DaemonServiceState::Stopped => (
            "索引服务当前已停止",
            format!(
                "本地已安装 goz v{PINNED_GOZ_VERSION} Windows 系统服务，但当前处于停止状态。启动服务后即可恢复毫秒级全盘搜索与实时索引监听。"
            ),
            ValueTone::Warn,
            icons::POWER,
        ),
        DaemonServiceState::Disabled => (
            "索引服务当前已被禁用",
            format!(
                "本地已检测到 goz v{PINNED_GOZ_VERSION} Windows 系统服务，但服务启动类型已被禁用。点击「启动服务」可通过管理员权限重新启用并拉起服务。"
            ),
            ValueTone::Warn,
            icons::ALERT_TRIANGLE,
        ),
        DaemonServiceState::Paused => (
            "索引服务已暂停",
            format!(
                "本地 goz v{PINNED_GOZ_VERSION} 服务处于暂停状态，恢复运行后即可继续提供实时搜索。"
            ),
            ValueTone::Warn,
            icons::POWER,
        ),
        DaemonServiceState::Starting | DaemonServiceState::Stopping | DaemonServiceState::Running => (
            "正在等待索引服务就绪…",
            format!(
                "Windows 服务管理器正在切换 goz v{PINNED_GOZ_VERSION} 服务状态，稍候即可建立安全管道连接。"
            ),
            ValueTone::Info,
            icons::HARD_DRIVE,
        ),
        DaemonServiceState::UnsupportedPlatform => (
            "当前系统不支持 NTFS MFT 服务",
            format!(
                "goz v{PINNED_GOZ_VERSION} 后台索引服务仅支持 Windows NTFS 卷。当前可在非 Windows 桌面编译与预览界面，全盘索引与服务管理需在 Windows 上运行。"
            ),
            ValueTone::Neutral,
            icons::ALERT_CIRCLE,
        ),
    };

    let primary_cta = match svc {
        DaemonServiceState::NotInstalled => action_busy(
            ButtonKind::Primary,
            format!("一键安装并启动服务 (goz v{PINNED_GOZ_VERSION})"),
            store::service_action_busy,
            store::install_or_upgrade_service_action,
        )
        .into_element(),
        DaemonServiceState::Stopped | DaemonServiceState::Disabled | DaemonServiceState::Paused => {
            action_busy(
                ButtonKind::Primary,
                "启动服务",
                store::service_action_busy,
                store::start_service_action,
            )
            .into_element()
        }
        DaemonServiceState::Starting | DaemonServiceState::Stopping | DaemonServiceState::Running => {
            action_busy(
                ButtonKind::Primary,
                "重新检测",
                store::service_action_busy,
                store::reconnect_or_refresh,
            )
            .into_element()
        }
        DaemonServiceState::UnsupportedPlatform => action_disabled(
            ButtonKind::Secondary,
            "仅支持 Windows 服务",
            || true,
            || {},
        )
        .into_element(),
    };

    let show_secondary_refresh = !matches!(
        svc,
        DaemonServiceState::Starting | DaemonServiceState::Stopping | DaemonServiceState::Running
    );

    onboarding_card_shell(
        icon_markup,
        badge_tone,
        title,
        svc.label(),
        body,
        primary_cta,
        show_secondary_refresh,
    )
}

fn untrusted_card() -> impl IntoElement {
    let primary_cta = action_busy(
        ButtonKind::Primary,
        format!("安装 / 修复服务 (goz v{PINNED_GOZ_VERSION})"),
        store::service_action_busy,
        store::install_or_upgrade_service_action,
    )
    .into_element();

    onboarding_card_shell(
        icons::ALERT_TRIANGLE,
        ValueTone::Bad,
        "索引服务管道安全校验未通过",
        "Untrusted",
        "检测到命名管道 \\\\.\\pipe\\goz-v1 的所有者并非 SYSTEM 或 Administrators。为确保系统安全，Ramen 已拦截此连接；请重新安装官方校验的 goz v0.7.1 服务。".to_owned(),
        primary_cta,
        true,
    )
}

fn error_card(msg: String) -> impl IntoElement {
    let primary_cta = action_busy(
        ButtonKind::Primary,
        "重新检测",
        store::service_action_busy,
        store::reconnect_or_refresh,
    )
    .into_element();

    onboarding_card_shell(
        icons::ALERT_CIRCLE,
        ValueTone::Bad,
        "无法连接到 goz 索引服务",
        "Error",
        format!(
            "连接 goz v{PINNED_GOZ_VERSION} 守护进程时发生异常：{msg}。你可以先点击「重新检测」，或在服务异常时打开设置重新安装服务。"
        ),
        primary_cta,
        false,
    )
}

fn onboarding_card_shell(
    icon_markup: &'static str,
    tone: ValueTone,
    title: &'static str,
    badge_label: &'static str,
    body: String,
    primary_cta: AnyElement,
    show_secondary_refresh: bool,
) -> impl IntoElement {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .p_6()
        .child(
            card()
                .w_full()
                .max_w(px(540.0))
                .p_6()
                .gap_4()
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
                                .flex_row()
                                .items_center()
                                .gap_2p5()
                                .child(icons::svg_icon(icon_markup, 20.0, move || tone.color()))
                                .child(
                                    text(title)
                                        .role(AccessibilityRole::Header)
                                        .text_base()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(move || theme().foreground),
                                ),
                        )
                        .child(status_badge(badge_label, tone)),
                )
                .child(
                    text(body)
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                )
                .child(when(store::service_action_busy, || {
                    view()
                        .px_3()
                        .py_2()
                        .rounded(crate::ui::radius)
                        .border_1()
                        .border_color(move || theme().border)
                        .bg(move || theme().muted)
                        .child(
                            text(move || {
                                store::service_action_label()
                                    .unwrap_or_else(|| "正在处理服务请求…".to_owned())
                            })
                            .text_xs()
                            .text_color(move || theme().foreground),
                        )
                }))
                .child(when(
                    move || store::service_action_error().is_some(),
                    || {
                        view()
                            .px_3()
                            .py_2()
                            .rounded(crate::ui::radius)
                            .border_1()
                            .border_color(move || theme().destructive)
                            .bg(move || theme().muted)
                            .child(
                                text(move || {
                                    store::service_action_error().unwrap_or_default()
                                })
                                .text_xs()
                                .text_color(move || theme().destructive),
                            )
                    },
                ))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(primary_cta)
                        .child(when(
                            move || show_secondary_refresh,
                            || {
                                action_busy(
                                    ButtonKind::Secondary,
                                    "重新检测",
                                    store::service_action_busy,
                                    store::reconnect_or_refresh,
                                )
                            },
                        ))
                        .child(action(ButtonKind::Ghost, "打开服务设置…", || {
                            store::set_settings_open(true)
                        })),
                ),
        )
}
