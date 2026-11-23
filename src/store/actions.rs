//! Store actions: debounced `SearchEngine` dispatch, 32ms event polling,
//! UAC service management (`gozd` v0.7.1), file/clipboard launchers, and release checks.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bento::updater::{check_latest_release, UpdateStatus};
use nsh_core::prelude::*;

use super::{
    apply_search_outcome, current_engine, current_search_params, ensure_engine, is_service_busy,
    is_test_seeded, is_update_checking, notices, record_generation, set_daemon_connection,
    set_daemon_status, set_query_error, set_service_busy, set_update_status, take_engine,
    SearchRow,
};
use crate::goz::{
    ensure_gozd_binary, install_or_upgrade_service, installed_gozd_path, locate_gozd_binary,
    start_service, uninstall_service, EngineEvent,
};

thread_local! {
    static PENDING_SEARCH_TIMER: RefCell<Option<Spawned<()>>> = const { RefCell::new(None) };
}

fn cancel_pending_search_timer() {
    PENDING_SEARCH_TIMER.with(|cell| {
        if let Some(prev) = cell.borrow_mut().take() {
            prev.cancel();
        }
    });
}

/// Starts the `32ms` `SearchEngine` event queue poller under the current shell owner.
///
/// When [`super::seed_results_for_test`] or [`super::seed_offline_for_test`] has already
/// seeded deterministic test data, polling is skipped so host SCM/daemon state cannot
/// overwrite headless UI tests.
pub fn start_engine_poller() {
    if is_test_seeded() {
        return;
    }

    let engine = ensure_engine();
    engine.lock().probe_daemon();

    set_interval(Duration::from_millis(32), || {
        if is_test_seeded() {
            return;
        }
        let events = match current_engine() {
            Some(engine) => engine.lock().drain_events(),
            None => return,
        };

        for event in events {
            match event {
                EngineEvent::DaemonState(conn) => set_daemon_connection(conn),
                EngineEvent::StatusUpdated(status) => set_daemon_status(status),
                EngineEvent::SearchResults {
                    generation,
                    outcome,
                } => apply_search_outcome(generation, outcome),
                EngineEvent::SearchRefused {
                    generation,
                    message,
                } => set_query_error(generation, format!("查询语法错误：{message}")),
                EngineEvent::SearchFailed {
                    generation,
                    message,
                } => set_query_error(generation, format!("搜索请求失败：{message}")),
                EngineEvent::IndexChanged { .. } => {
                    // `SearchEngine` automatically queues status + optional search refresh on IndexChanged.
                }
            }
        }
    });
}

/// Cancels any pending search debounce timer and shuts down `SearchEngine` worker threads.
pub fn shutdown_engine() {
    cancel_pending_search_timer();
    if let Some(engine) = take_engine() {
        engine.lock().shutdown();
    }
}

/// Schedules a search after a 35ms typing debounce, cancelling any older pending timer.
pub fn schedule_search() {
    cancel_pending_search_timer();
    let task = set_timeout(Duration::from_millis(35), || {
        PENDING_SEARCH_TIMER.with(|cell| {
            *cell.borrow_mut() = None;
        });
        submit_search_immediate();
    });
    PENDING_SEARCH_TIMER.with(|cell| {
        *cell.borrow_mut() = Some(task);
    });
}

/// Submits the current query, category, scope, match_case, and sort immediately to `SearchEngine`.
pub fn submit_search_immediate() {
    cancel_pending_search_timer();
    if is_test_seeded() {
        super::refresh_seeded_results();
        return;
    }
    if !super::daemon_state_untracked().is_connected() {
        return;
    }

    let params = current_search_params();
    let engine = ensure_engine();
    let generation = engine.lock().submit_search(params);
    record_generation(generation);
}

/// Re-probes the daemon connection and refreshes daemon status and active search results.
pub fn reconnect_or_refresh() {
    if is_test_seeded() {
        super::refresh_seeded_results();
        return;
    }

    let connected = super::daemon_state_untracked().is_connected();
    let engine = ensure_engine();
    let guard = engine.lock();
    guard.probe_daemon();
    if connected {
        guard.refresh_search();
    }
}

/// Downloads/verifies `gozd v0.7.1` (if missing) and installs or upgrades + starts the Windows service.
pub fn install_or_upgrade_service_action() {
    if is_service_busy() {
        return;
    }

    let initial_label = if locate_gozd_binary().is_some() {
        "正在请求管理员权限安装并启动 goz v0.7.1 服务…".to_owned()
    } else {
        "正在下载 gozd v0.7.1 并校验 SHA-256…".to_owned()
    };
    set_service_busy(true, Some(initial_label), None);

    spawn(async move {
        let outcome = blocking(move || {
            let gozd_exe = ensure_gozd_binary()?;
            install_or_upgrade_service(&gozd_exe)
        })
        .await;

        match outcome {
            Ok(Ok(())) => {
                set_service_busy(false, None, None);
                notices().notify_ok("goz v0.7.1 索引服务已安装并启动");
                if !is_test_seeded() {
                    ensure_engine().lock().probe_daemon();
                    // Give the newly started daemon a short window to create its named pipe if still warming up.
                    timer(Duration::from_millis(600)).await;
                    if !super::daemon_state_untracked().is_connected() {
                        if let Some(engine) = current_engine() {
                            engine.lock().probe_daemon();
                        }
                    }
                }
            }
            Ok(Err(err)) => {
                set_service_busy(false, None, Some(err.clone()));
                notices().notify_bad(format!("安装或升级服务失败：{err}"));
            }
            Err(err) => {
                let msg = err.to_string();
                set_service_busy(false, None, Some(msg.clone()));
                notices().notify_bad(format!("服务任务异常：{msg}"));
            }
        }
    });
}

/// Enables and starts an already-installed `goz` Windows service via UAC elevation.
pub fn start_service_action() {
    if is_service_busy() {
        return;
    }

    set_service_busy(
        true,
        Some("正在请求管理员权限启动 goz 索引服务…".to_owned()),
        None,
    );

    spawn(async move {
        let outcome = blocking(start_service).await;

        match outcome {
            Ok(Ok(())) => {
                set_service_busy(false, None, None);
                notices().notify_ok("goz 索引服务已启动");
                if !is_test_seeded() {
                    ensure_engine().lock().probe_daemon();
                    timer(Duration::from_millis(600)).await;
                    if !super::daemon_state_untracked().is_connected() {
                        if let Some(engine) = current_engine() {
                            engine.lock().probe_daemon();
                        }
                    }
                }
            }
            Ok(Err(err)) => {
                set_service_busy(false, None, Some(err.clone()));
                notices().notify_bad(format!("启动服务失败：{err}"));
            }
            Err(err) => {
                let msg = err.to_string();
                set_service_busy(false, None, Some(msg.clone()));
                notices().notify_bad(format!("启动任务异常：{msg}"));
            }
        }
    });
}

/// Stops and uninstalls the `goz` Windows service via UAC elevation.
pub fn uninstall_service_action() {
    if is_service_busy() {
        return;
    }

    set_service_busy(
        true,
        Some("正在请求管理员权限停止并卸载 goz 服务…".to_owned()),
        None,
    );

    spawn(async move {
        let outcome = blocking(move || {
            let exe = installed_gozd_path()
                .filter(|p| p.is_file())
                .or_else(locate_gozd_binary)
                .unwrap_or_else(PathBuf::new);
            uninstall_service(&exe)
        })
        .await;

        match outcome {
            Ok(Ok(())) => {
                set_service_busy(false, None, None);
                notices().notify_ok("已卸载 goz 索引服务");
                if !is_test_seeded() {
                    ensure_engine().lock().probe_daemon();
                }
            }
            Ok(Err(err)) => {
                set_service_busy(false, None, Some(err.clone()));
                notices().notify_bad(format!("卸载服务失败：{err}"));
            }
            Err(err) => {
                let msg = err.to_string();
                set_service_busy(false, None, Some(msg.clone()));
                notices().notify_bad(format!("卸载任务异常：{msg}"));
            }
        }
    });
}

/// Opens a matched file or directory using the system default handler.
pub fn open_entry(row: &SearchRow) {
    let path = Path::new(row.item.path.as_str());
    let res = if row.item.is_dir {
        bento::shell::open_directory(path)
    } else {
        bento::shell::open_local_file(path)
    };
    if let Err(err) = res {
        notices().notify_bad(format!("无法打开 {}：{err}", row.file_name));
    }
}

/// Reveals and selects a matched entry in the system file manager.
pub fn reveal_entry(row: &SearchRow) {
    if let Err(err) = bento::shell::reveal_in_file_manager(Path::new(row.item.path.as_str())) {
        notices().notify_bad(format!("无法在文件管理器中显示：{err}"));
    }
}

/// Copies `value` to the system clipboard and displays a confirmation toast.
pub fn copy_path(value: &str, kind_label: &str) {
    set_clipboard(value);
    notices().notify_ok(format!("已复制{kind_label}：{value}"));
}

/// Checks GitHub Releases for a newer Ramen version without blocking the UI thread.
pub fn check_for_updates() {
    if is_update_checking() {
        return;
    }
    set_update_status(UpdateStatus::Checking);

    spawn(async move {
        let result = check_latest_release(crate::APP_META, None).await;
        set_update_status(UpdateStatus::from(result));
    });
}
