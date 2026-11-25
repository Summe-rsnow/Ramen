//! Centralized reactive store for Ramen.
//!
//! All signals are strictly private to this module (zero `pub Signal` exposure).
//! Views and dialogs read state via tracked getters and dispatch mutations via
//! semantic actions in this module and [`actions`].

mod actions;

pub use actions::{
    check_for_updates, copy_path, install_or_upgrade_service_action, open_entry,
    reconnect_or_refresh, reveal_entry, shutdown_engine, start_engine_poller,
    start_service_action, submit_search_immediate, uninstall_service_action,
};

use std::collections::HashMap;
use std::sync::Arc;

use bento::hotkey::{HotkeyBinding, HotkeyStatus};
use bento::shell::{CloseBehavior, NoticeCenter, ThemeMode};
use bento::updater::UpdateStatus;
use nsh_core::prelude::*;
use nsh_core::storage::Storage;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::goz::{
    compose_query, detect_file_kind, split_path, DaemonConnectionState, DaemonServiceState,
    DaemonStatus, FileCategory, FileIconKind, ResultItem, SearchEngine, SortDir, SortKey, SortSpec,
};

const PREFS_STORAGE_KEY: &str = "ramen.prefs.v1";

/// UI-facing connection state for the `gozd` daemon (including initial [`Self::Connecting`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DaemonUiState {
    #[default]
    Connecting,
    Connected {
        server: String,
        ready: bool,
        version_matches: bool,
        supports_watch: bool,
        features: Vec<String>,
    },
    Offline(DaemonServiceState),
    Untrusted,
    Error(String),
}

impl DaemonUiState {
    /// Converts a backend [`DaemonConnectionState`] into [`DaemonUiState`].
    pub fn from_connection(conn: DaemonConnectionState) -> Self {
        match conn {
            DaemonConnectionState::Connected {
                server,
                ready,
                version_matches,
                supports_watch,
                features,
            } => Self::Connected {
                server,
                ready,
                version_matches,
                supports_watch,
                features,
            },
            DaemonConnectionState::Offline(svc) => Self::Offline(svc),
            DaemonConnectionState::Untrusted => Self::Untrusted,
            DaemonConnectionState::Error(msg) => Self::Error(msg),
        }
    }

    /// Returns `true` when connected to `gozd`.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }

    /// Returns `Some(server)` when connected to a `gozd` binary whose version differs from `0.7.1`.
    pub fn mismatched_server(&self) -> Option<&str> {
        match self {
            Self::Connected {
                server,
                version_matches: false,
                ..
            } => Some(server.as_str()),
            _ => None,
        }
    }
}

/// Pre-processed row model for `virtual_list` rendering without per-frame string splitting.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchRow {
    pub item: ResultItem,
    pub parent_dir: SharedString,
    pub file_name: SharedString,
    pub icon_kind: FileIconKind,
    pub size_label: SharedString,
    pub mtime_label: SharedString,
}

impl SearchRow {
    /// Builds a [`SearchRow`] from a `goz` [`ResultItem`], splitting path and formatting size/time once.
    pub fn from_item(item: ResultItem) -> Self {
        let (parent, name) = split_path(&item.path);
        let icon_kind = detect_file_kind(name, item.is_dir);
        let size_label: SharedString = match item.size {
            Some(bytes) => bento::format::format_bytes(bytes).into(),
            None => "—".into(),
        };
        let mtime_label: SharedString = item
            .mtime_ft
            .and_then(bento::format::format_filetime_local)
            .map(SharedString::from)
            .unwrap_or_else(|| "—".into());

        Self {
            parent_dir: SharedString::from(parent.to_owned()),
            file_name: SharedString::from(name.to_owned()),
            icon_kind,
            size_label,
            mtime_label,
            item,
        }
    }
}

/// Persisted user preferences stored under [`PREFS_STORAGE_KEY`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RamenPrefs {
    pub theme_mode: ThemeMode,
    pub close_behavior: CloseBehavior,
    pub global_hotkey: HotkeyBinding,
    pub match_case: bool,
    pub auto_refresh: bool,
}

impl Default for RamenPrefs {
    fn default() -> Self {
        Self {
            theme_mode: ThemeMode::System,
            close_behavior: CloseBehavior::Ask,
            global_hotkey: HotkeyBinding::new(true, "alt+space"),
            match_case: false,
            auto_refresh: true,
        }
    }
}

// --- Private signal wrapper types ---

#[derive(Default, Clone, PartialEq)]
struct QueryText(String);

#[derive(Default, Clone, PartialEq)]
struct SelectedCategory(FileCategory);

#[derive(Default, Clone, PartialEq)]
struct ScopeFilter(Option<String>);

#[derive(Default, Clone, PartialEq)]
struct MatchCase(bool);

#[derive(Default, Clone, PartialEq)]
struct SortState(SortSpec);

#[derive(Default, Clone, PartialEq)]
struct ResultsState(Vec<SearchRow>);

#[derive(Default, Clone, PartialEq)]
struct TotalMatches(u64);

#[derive(Default, Clone, PartialEq)]
struct TotalSize(Option<u64>);

#[derive(Default, Clone, PartialEq)]
struct TruncatedState(bool);

#[derive(Default, Clone, PartialEq)]
struct ElapsedMs(Option<u64>);

#[derive(Default, Clone, PartialEq)]
struct VolumesIncomplete(bool);

#[derive(Default, Clone, PartialEq)]
struct MetadataPending(bool);

#[derive(Default, Clone, PartialEq)]
struct QueryError(Option<String>);

#[derive(Default, Clone, PartialEq)]
struct SelectedIndex(Option<usize>);

#[derive(Default, Clone, PartialEq)]
struct DaemonConnState(DaemonUiState);

#[derive(Default, Clone, PartialEq)]
struct DaemonStatusState(Option<DaemonStatus>);

#[derive(Clone, PartialEq)]
struct AutoRefreshState(bool);

impl Default for AutoRefreshState {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Default, Clone, PartialEq)]
struct AutoStartState(bool);

#[derive(Default, Clone, PartialEq)]
struct ServiceActionBusy(bool);

#[derive(Default, Clone, PartialEq)]
struct ServiceActionLabel(Option<String>);

#[derive(Default, Clone, PartialEq)]
struct ServiceActionError(Option<String>);

#[derive(Default, Clone, PartialEq)]
struct ThemeModeState(ThemeMode);

#[derive(Default, Clone, PartialEq)]
struct CloseBehaviorState(CloseBehavior);

#[derive(Clone, PartialEq)]
struct GlobalHotkeyState(HotkeyBinding);

impl Default for GlobalHotkeyState {
    fn default() -> Self {
        Self(RamenPrefs::default().global_hotkey)
    }
}

#[derive(Clone, PartialEq)]
struct HotkeyStatusState(HotkeyStatus);

impl Default for HotkeyStatusState {
    fn default() -> Self {
        Self(HotkeyStatus::Disabled)
    }
}

#[derive(Default, Clone, PartialEq)]
struct SettingsOpen(bool);

#[derive(Default, Clone, PartialEq)]
struct AboutOpen(bool);

#[derive(Default, Clone, PartialEq)]
struct SyntaxHelpOpen(bool);

#[derive(Default, Clone, PartialEq)]
struct UninstallConfirmOpen(bool);

#[derive(Default, Clone, PartialEq)]
struct UpdateStatusState(UpdateStatus);

#[derive(Default, Clone, PartialEq)]
struct TestSeededState(bool);

#[derive(Default, Clone, PartialEq)]
struct SeededDatasetState(Option<Vec<ResultItem>>);

#[derive(Default, Clone, PartialEq)]
struct SearchGenState(u64);

#[derive(Default, Clone, PartialEq)]
struct HasSearchedInitial(bool);

#[derive(Default, Clone)]
struct QueryTimersMap(Arc<Mutex<HashMap<u64, std::time::Instant>>>);

impl PartialEq for QueryTimersMap {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, Default)]
struct EngineHandle(Option<Arc<Mutex<SearchEngine>>>);

impl PartialEq for EngineHandle {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

// --- Private signal accessors ---

fn query_sig() -> Signal<QueryText> {
    global::<QueryText>()
}
fn category_sig() -> Signal<SelectedCategory> {
    global::<SelectedCategory>()
}
fn scope_sig() -> Signal<ScopeFilter> {
    global::<ScopeFilter>()
}
fn match_case_sig() -> Signal<MatchCase> {
    global::<MatchCase>()
}
fn sort_sig() -> Signal<SortState> {
    global::<SortState>()
}
fn results_sig() -> Signal<ResultsState> {
    global::<ResultsState>()
}
fn total_matches_sig() -> Signal<TotalMatches> {
    global::<TotalMatches>()
}
fn total_size_sig() -> Signal<TotalSize> {
    global::<TotalSize>()
}
fn truncated_sig() -> Signal<TruncatedState> {
    global::<TruncatedState>()
}
fn elapsed_ms_sig() -> Signal<ElapsedMs> {
    global::<ElapsedMs>()
}
fn volumes_incomplete_sig() -> Signal<VolumesIncomplete> {
    global::<VolumesIncomplete>()
}
fn metadata_pending_sig() -> Signal<MetadataPending> {
    global::<MetadataPending>()
}
fn query_error_sig() -> Signal<QueryError> {
    global::<QueryError>()
}
fn selected_index_sig() -> Signal<SelectedIndex> {
    global::<SelectedIndex>()
}
fn daemon_conn_sig() -> Signal<DaemonConnState> {
    global::<DaemonConnState>()
}
fn daemon_status_sig() -> Signal<DaemonStatusState> {
    global::<DaemonStatusState>()
}
fn auto_refresh_sig() -> Signal<AutoRefreshState> {
    global::<AutoRefreshState>()
}
fn autostart_sig() -> Signal<AutoStartState> {
    global::<AutoStartState>()
}
fn service_busy_sig() -> Signal<ServiceActionBusy> {
    global::<ServiceActionBusy>()
}
fn service_label_sig() -> Signal<ServiceActionLabel> {
    global::<ServiceActionLabel>()
}
fn service_error_sig() -> Signal<ServiceActionError> {
    global::<ServiceActionError>()
}
fn theme_mode_sig() -> Signal<ThemeModeState> {
    global::<ThemeModeState>()
}
fn close_behavior_sig() -> Signal<CloseBehaviorState> {
    global::<CloseBehaviorState>()
}
fn global_hotkey_sig() -> Signal<GlobalHotkeyState> {
    global::<GlobalHotkeyState>()
}
fn hotkey_status_sig() -> Signal<HotkeyStatusState> {
    global::<HotkeyStatusState>()
}
fn settings_open_sig() -> Signal<SettingsOpen> {
    global::<SettingsOpen>()
}
fn about_open_sig() -> Signal<AboutOpen> {
    global::<AboutOpen>()
}
fn syntax_help_open_sig() -> Signal<SyntaxHelpOpen> {
    global::<SyntaxHelpOpen>()
}
fn uninstall_confirm_open_sig() -> Signal<UninstallConfirmOpen> {
    global::<UninstallConfirmOpen>()
}
fn update_status_sig() -> Signal<UpdateStatusState> {
    global::<UpdateStatusState>()
}
fn test_seeded_sig() -> Signal<TestSeededState> {
    global::<TestSeededState>()
}
fn seeded_dataset_sig() -> Signal<SeededDatasetState> {
    global::<SeededDatasetState>()
}
fn search_gen_sig() -> Signal<SearchGenState> {
    global::<SearchGenState>()
}
fn has_searched_initial_sig() -> Signal<HasSearchedInitial> {
    global::<HasSearchedInitial>()
}
fn query_timers_sig() -> Signal<QueryTimersMap> {
    global::<QueryTimersMap>()
}
fn engine_sig() -> Signal<EngineHandle> {
    global::<EngineHandle>()
}

// --- Internal helpers shared with `actions` ---

fn ensure_engine() -> Arc<Mutex<SearchEngine>> {
    if let Some(handle) = engine_sig().with_untracked(|h| h.0.clone()) {
        return handle;
    }
    let engine = SearchEngine::new();
    engine.set_auto_refresh(auto_refresh_sig().get_untracked().0);
    let handle = Arc::new(Mutex::new(engine));
    engine_sig().set(EngineHandle(Some(Arc::clone(&handle))));
    handle
}

fn current_engine() -> Option<Arc<Mutex<SearchEngine>>> {
    engine_sig().with_untracked(|h| h.0.clone())
}

fn take_engine() -> Option<Arc<Mutex<SearchEngine>>> {
    let existing = current_engine();
    if existing.is_some() {
        engine_sig().set(EngineHandle(None));
    }
    existing
}

fn is_test_seeded() -> bool {
    test_seeded_sig().get_untracked().0
}

fn current_search_params() -> crate::goz::SearchParams {
    crate::goz::SearchParams {
        raw_query: query_sig().get_untracked().0,
        category: category_sig().get_untracked().0,
        scope: scope_sig().get_untracked().0,
        sort: sort_sig().get_untracked().0,
        match_case: match_case_sig().get_untracked().0,
        max_items: 2000,
        want_size: true,
        want_mtime: true,
    }
}

fn record_generation(generation: u64) {
    search_gen_sig().set(SearchGenState(generation));
    let timers = query_timers_sig().with_untracked(|m| Arc::clone(&m.0));
    let mut guard = timers.lock();
    guard.retain(|g, _| generation.saturating_sub(*g) < 32);
    guard.insert(generation, std::time::Instant::now());
}

fn take_generation_elapsed_ms(generation: u64) -> Option<u64> {
    let timers = query_timers_sig().with_untracked(|m| Arc::clone(&m.0));
    let started = timers.lock().remove(&generation)?;
    Some(started.elapsed().as_millis() as u64)
}

fn current_generation() -> u64 {
    search_gen_sig().get_untracked().0
}

fn set_daemon_connection(conn: DaemonConnectionState) {
    let ui_state = DaemonUiState::from_connection(conn);
    let connected = ui_state.is_connected();
    let should_kick_initial = connected && !has_searched_initial_sig().get_untracked().0;

    batch(|| {
        daemon_conn_sig().set(DaemonConnState(ui_state));
        if !connected {
            daemon_status_sig().set(DaemonStatusState(None));
        }
        if should_kick_initial {
            has_searched_initial_sig().set(HasSearchedInitial(true));
        }
    });

    if should_kick_initial {
        submit_search_immediate();
    }
}

fn set_daemon_status(status: DaemonStatus) {
    daemon_status_sig().set(DaemonStatusState(Some(status)));
}

fn apply_search_outcome(generation: u64, outcome: crate::goz::QueryOutcome) {
    if generation != current_generation() && current_generation() != 0 {
        return;
    }
    let elapsed = take_generation_elapsed_ms(generation);
    let prev_selected_path = selected_index_sig()
        .get_untracked()
        .0
        .and_then(|idx| results_sig().with_untracked(|r| r.0.get(idx).map(|row| row.item.path.clone())));

    let rows: Vec<SearchRow> = outcome.items.into_iter().map(SearchRow::from_item).collect();
    let total_size = outcome.total_size.or_else(|| {
        let has_any = rows.iter().any(|r| r.item.size.is_some());
        has_any.then(|| rows.iter().filter_map(|r| r.item.size).sum())
    });

    let next_selected = prev_selected_path.and_then(|path| {
        rows.iter().position(|r| r.item.path == path)
    });

    batch(|| {
        results_sig().set(ResultsState(rows));
        total_matches_sig().set(TotalMatches(outcome.total));
        total_size_sig().set(TotalSize(total_size));
        truncated_sig().set(TruncatedState(outcome.truncated));
        elapsed_ms_sig().set(ElapsedMs(elapsed));
        volumes_incomplete_sig().set(VolumesIncomplete(outcome.volumes_incomplete));
        metadata_pending_sig().set(MetadataPending(outcome.metadata_pending));
        query_error_sig().set(QueryError(None));
        selected_index_sig().set(SelectedIndex(next_selected));
    });
}

fn set_query_error(generation: u64, message: String) {
    if generation != current_generation() && current_generation() != 0 {
        return;
    }
    query_error_sig().set(QueryError(Some(message)));
}

fn set_service_busy(busy: bool, label: Option<String>, error: Option<String>) {
    batch(|| {
        service_busy_sig().set(ServiceActionBusy(busy));
        service_label_sig().set(ServiceActionLabel(label));
        service_error_sig().set(ServiceActionError(error));
    });
}

fn is_service_busy() -> bool {
    service_busy_sig().get_untracked().0
}

fn is_update_checking() -> bool {
    matches!(
        update_status_sig().get_untracked().0,
        UpdateStatus::Checking
    )
}

fn set_update_status(status: UpdateStatus) {
    update_status_sig().set(UpdateStatusState(status));
}

fn persist_prefs() {
    let prefs = RamenPrefs {
        theme_mode: theme_mode_sig().get_untracked().0,
        close_behavior: close_behavior_sig().get_untracked().0,
        global_hotkey: global_hotkey_sig().get_untracked().0,
        match_case: match_case_sig().get_untracked().0,
        auto_refresh: auto_refresh_sig().get_untracked().0,
    };
    if let Ok(json) = serde_json::to_string(&prefs) {
        if let Err(err) = Storage::set_item(PREFS_STORAGE_KEY, json) {
            log::warn!("Failed to persist Ramen prefs: {err}");
        }
    }
}

/// Initializes store state on app startup from `Storage` without blocking the UI thread.
pub fn load() {
    if is_test_seeded() {
        return;
    }
    let prefs = Storage::get_item(PREFS_STORAGE_KEY)
        .and_then(|raw| serde_json::from_str::<RamenPrefs>(&raw).ok())
        .unwrap_or_default();

    let autostart = match bento::autostart::is_autostart_enabled(crate::APP_META.name) {
        Ok(enabled) => enabled,
        Err(err) => {
            log::warn!("Failed to query autostart state: {err}");
            false
        }
    };

    batch(|| {
        theme_mode_sig().set(ThemeModeState(prefs.theme_mode));
        close_behavior_sig().set(CloseBehaviorState(prefs.close_behavior));
        global_hotkey_sig().set(GlobalHotkeyState(prefs.global_hotkey));
        match_case_sig().set(MatchCase(prefs.match_case));
        auto_refresh_sig().set(AutoRefreshState(prefs.auto_refresh));
        autostart_sig().set(AutoStartState(autostart));
        daemon_conn_sig().set(DaemonConnState(DaemonUiState::Connecting));
    });

    prefs.theme_mode.apply();
}

// --- Test Seeding Helpers (for `tests/screen.rs` headless UI integration tests) ---

/// Seeds connected search results and daemon status for deterministic headless UI tests.
#[doc(hidden)]
pub fn seed_results_for_test(
    items: Vec<ResultItem>,
    status: DaemonStatus,
    version_matches: bool,
    volumes_incomplete: bool,
    metadata_pending: bool,
) {
    let rows: Vec<SearchRow> = items.iter().cloned().map(SearchRow::from_item).collect();
    let total = rows.len() as u64;
    let total_size = {
        let has_any = rows.iter().any(|r| r.item.size.is_some());
        has_any.then(|| rows.iter().filter_map(|r| r.item.size).sum())
    };
    let server = if version_matches {
        "gozd 0.7.1".to_owned()
    } else {
        "gozd 0.6.0".to_owned()
    };

    batch(|| {
        test_seeded_sig().set(TestSeededState(true));
        seeded_dataset_sig().set(SeededDatasetState(Some(items)));
        autostart_sig().set(AutoStartState(false));
        daemon_conn_sig().set(DaemonConnState(DaemonUiState::Connected {
            server,
            ready: !volumes_incomplete,
            version_matches,
            supports_watch: true,
            features: vec!["watch".to_owned()],
        }));
        daemon_status_sig().set(DaemonStatusState(Some(status)));
        results_sig().set(ResultsState(rows));
        total_matches_sig().set(TotalMatches(total));
        total_size_sig().set(TotalSize(total_size));
        truncated_sig().set(TruncatedState(false));
        elapsed_ms_sig().set(ElapsedMs(Some(2)));
        volumes_incomplete_sig().set(VolumesIncomplete(volumes_incomplete));
        metadata_pending_sig().set(MetadataPending(metadata_pending));
        query_error_sig().set(QueryError(None));
        selected_index_sig().set(SelectedIndex(None));
    });
}

/// Seeds an offline/service-missing daemon state for testing service onboarding without touching SCM.
#[doc(hidden)]
pub fn seed_offline_for_test(service_state: DaemonServiceState) {
    batch(|| {
        test_seeded_sig().set(TestSeededState(true));
        seeded_dataset_sig().set(SeededDatasetState(None));
        autostart_sig().set(AutoStartState(false));
        daemon_conn_sig().set(DaemonConnState(DaemonUiState::Offline(service_state)));
        daemon_status_sig().set(DaemonStatusState(None));
        results_sig().set(ResultsState(Vec::new()));
        total_matches_sig().set(TotalMatches(0));
        total_size_sig().set(TotalSize(None));
        truncated_sig().set(TruncatedState(false));
        elapsed_ms_sig().set(ElapsedMs(None));
        volumes_incomplete_sig().set(VolumesIncomplete(false));
        metadata_pending_sig().set(MetadataPending(false));
        query_error_sig().set(QueryError(None));
        selected_index_sig().set(SelectedIndex(None));
    });
}

fn refresh_seeded_results() {
    let Some(dataset) = seeded_dataset_sig().get_untracked().0 else {
        return;
    };
    let raw_query = query_sig().get_untracked().0;
    let category = category_sig().get_untracked().0;
    let scope = scope_sig().get_untracked().0;
    let match_case = match_case_sig().get_untracked().0;
    let sort = sort_sig().get_untracked().0;

    let mut filtered: Vec<ResultItem> = dataset
        .into_iter()
        .filter(|item| matches_category(item, category))
        .filter(|item| match scope.as_deref() {
            Some(prefix) if !prefix.is_empty() => item
                .path
                .to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase()),
            _ => true,
        })
        .filter(|item| matches_seeded_query(item, &raw_query, match_case))
        .collect();

    sort_seeded_items(&mut filtered, sort);

    let rows: Vec<SearchRow> = filtered.into_iter().map(SearchRow::from_item).collect();
    let total = rows.len() as u64;
    let total_size = {
        let has_any = rows.iter().any(|r| r.item.size.is_some());
        has_any.then(|| rows.iter().filter_map(|r| r.item.size).sum())
    };
    batch(|| {
        results_sig().set(ResultsState(rows));
        total_matches_sig().set(TotalMatches(total));
        total_size_sig().set(TotalSize(total_size));
        selected_index_sig().set(SelectedIndex(None));
        query_error_sig().set(QueryError(None));
    });
}

fn matches_category(item: &ResultItem, category: FileCategory) -> bool {
    let (_, name) = split_path(&item.path);
    let kind = detect_file_kind(name, item.is_dir);
    match category {
        FileCategory::All => true,
        FileCategory::Folders => item.is_dir,
        FileCategory::Documents => kind == FileIconKind::Document,
        FileCategory::Images => kind == FileIconKind::Image,
        FileCategory::Videos => kind == FileIconKind::Video,
        FileCategory::Audio => kind == FileIconKind::Audio,
        FileCategory::Archives => kind == FileIconKind::Archive,
        FileCategory::Code => kind == FileIconKind::Code,
    }
}

fn matches_seeded_query(item: &ResultItem, query: &str, match_case: bool) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return true;
    }
    let (_, name) = split_path(&item.path);
    for token in trimmed.split_whitespace() {
        if let Some(ext_spec) = token.strip_prefix("ext:") {
            let exts: Vec<&str> = ext_spec.split(';').filter(|s| !s.is_empty()).collect();
            let file_ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
            if !exts.iter().any(|e| file_ext.eq_ignore_ascii_case(e)) {
                return false;
            }
            continue;
        }
        if let Some(neg) = token.strip_prefix('!') {
            if !neg.is_empty() {
                let hit = if match_case {
                    item.path.contains(neg)
                } else {
                    item.path
                        .to_ascii_lowercase()
                        .contains(&neg.to_ascii_lowercase())
                };
                if hit {
                    return false;
                }
            }
            continue;
        }
        let needle = token.trim_matches('"');
        let hit = if match_case {
            item.path.contains(needle)
        } else {
            item.path
                .to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase())
        };
        if !hit {
            return false;
        }
    }
    true
}

fn sort_seeded_items(items: &mut [ResultItem], sort: SortSpec) {
    items.sort_by(|a, b| {
        let (pa, na) = split_path(&a.path);
        let (pb, nb) = split_path(&b.path);
        let cmp = match sort.key {
            SortKey::Name => na.to_ascii_lowercase().cmp(&nb.to_ascii_lowercase()),
            SortKey::Path => pa
                .to_ascii_lowercase()
                .cmp(&pb.to_ascii_lowercase())
                .then_with(|| na.to_ascii_lowercase().cmp(&nb.to_ascii_lowercase())),
            SortKey::Size => a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0)),
            SortKey::DateModified => a.mtime_ft.unwrap_or(0).cmp(&b.mtime_ft.unwrap_or(0)),
            _ => na.to_ascii_lowercase().cmp(&nb.to_ascii_lowercase()),
        };
        match sort.dir {
            SortDir::Asc => cmp,
            SortDir::Desc => cmp.reverse(),
        }
    });
}

fn trigger_search(debounced: bool) {
    if is_test_seeded() {
        refresh_seeded_results();
    } else if debounced {
        actions::schedule_search();
    } else {
        actions::submit_search_immediate();
    }
}

// --- Search Query, Filters, & Selection (tracked getters + setters) ---

pub fn query() -> String {
    query_sig().get().0
}

pub fn query_untracked() -> String {
    query_sig().get_untracked().0
}

pub fn set_query(next: impl Into<String>) {
    let next = next.into();
    if query_sig().get_untracked().0 == next {
        return;
    }
    query_sig().set(QueryText(next));
    trigger_search(true);
}

pub fn clear_query() {
    if query_sig().get_untracked().0.is_empty() {
        return;
    }
    query_sig().set(QueryText(String::new()));
    trigger_search(false);
}

pub fn composed_query_preview() -> String {
    compose_query(&query_sig().get().0, category_sig().get().0)
}

pub fn category() -> FileCategory {
    category_sig().get().0
}

pub fn set_category(cat: FileCategory) {
    let current = category_sig().get_untracked().0;
    let next = if current == cat && cat != FileCategory::All {
        FileCategory::All
    } else {
        cat
    };
    if next == current {
        return;
    }
    category_sig().set(SelectedCategory(next));
    trigger_search(false);
}

pub fn scope() -> Option<String> {
    scope_sig().get().0
}

pub fn set_scope(next: Option<String>) {
    let cleaned = next.and_then(|s| {
        let trimmed = s.trim().to_owned();
        (!trimmed.is_empty()).then_some(trimmed)
    });
    if scope_sig().get_untracked().0 == cleaned {
        return;
    }
    scope_sig().set(ScopeFilter(cleaned));
    trigger_search(false);
}

pub fn match_case() -> bool {
    match_case_sig().get().0
}

pub fn toggle_match_case() {
    let next = !match_case_sig().get_untracked().0;
    match_case_sig().set(MatchCase(next));
    persist_prefs();
    trigger_search(false);
}

pub fn sort() -> SortSpec {
    sort_sig().get().0
}

pub fn toggle_sort(key: SortKey) {
    let current = sort_sig().get_untracked().0;
    let next = if current.key == key {
        SortSpec {
            key,
            dir: match current.dir {
                SortDir::Asc => SortDir::Desc,
                SortDir::Desc => SortDir::Asc,
            },
        }
    } else {
        SortSpec::default_for(key)
    };
    sort_sig().set(SortState(next));
    trigger_search(false);
}

// --- Search Results & Selection ---

pub fn result_count() -> usize {
    results_sig().with(|r| r.0.len())
}

pub fn result_keys() -> Vec<String> {
    results_sig().with(|r| r.0.iter().map(|row| row.item.path.clone()).collect())
}

pub fn result_at(index: usize) -> Option<SearchRow> {
    results_sig().with(|r| r.0.get(index).cloned())
}

pub fn result_at_untracked(index: usize) -> Option<SearchRow> {
    results_sig().with_untracked(|r| r.0.get(index).cloned())
}

pub fn total_matches() -> u64 {
    total_matches_sig().get().0
}

pub fn total_size() -> Option<u64> {
    total_size_sig().get().0
}

pub fn is_truncated() -> bool {
    truncated_sig().get().0
}

pub fn elapsed_ms() -> Option<u64> {
    elapsed_ms_sig().get().0
}

pub fn volumes_incomplete() -> bool {
    volumes_incomplete_sig().get().0
        || daemon_status_sig()
            .with(|s| s.0.as_ref().is_some_and(|st| st.volumes_incomplete))
}

pub fn metadata_pending() -> bool {
    metadata_pending_sig().get().0
}

pub fn query_error() -> Option<String> {
    query_error_sig().get().0
}

pub fn selected_index() -> Option<usize> {
    selected_index_sig().get().0
}

pub fn selected_index_untracked() -> Option<usize> {
    selected_index_sig().get_untracked().0
}

pub fn set_selected_index(idx: Option<usize>) {
    let len = results_sig().with_untracked(|r| r.0.len());
    let clamped = idx.filter(|&i| i < len);
    selected_index_sig().set(SelectedIndex(clamped));
}

/// Moves the selected row by `delta` (`-1`, `+1`, `-12`, `+12`), returning the new index.
pub fn move_selection(delta: isize) -> Option<usize> {
    let len = results_sig().with_untracked(|r| r.0.len());
    if len == 0 {
        selected_index_sig().set(SelectedIndex(None));
        return None;
    }
    let next = match selected_index_sig().get_untracked().0 {
        None => {
            if delta >= 0 {
                0
            } else {
                len - 1
            }
        }
        Some(cur) => {
            let max = (len - 1) as isize;
            (cur as isize + delta).clamp(0, max) as usize
        }
    };
    selected_index_sig().set(SelectedIndex(Some(next)));
    Some(next)
}

pub fn selected_or_first_row() -> Option<SearchRow> {
    let idx = selected_index_sig().get_untracked().0.unwrap_or(0);
    result_at_untracked(idx)
}

pub fn selected_row() -> Option<SearchRow> {
    let idx = selected_index_sig().get_untracked().0?;
    result_at_untracked(idx)
}

// --- Daemon Connection & Service Management ---

pub fn daemon_state() -> DaemonUiState {
    daemon_conn_sig().get().0
}

pub fn daemon_state_untracked() -> DaemonUiState {
    daemon_conn_sig().get_untracked().0
}

pub fn is_connected() -> bool {
    daemon_conn_sig().with(|s| s.0.is_connected())
}

pub fn daemon_status() -> Option<DaemonStatus> {
    daemon_status_sig().get().0
}

pub fn total_indexed_entries() -> u64 {
    daemon_status_sig().with(|s| {
        s.0.as_ref()
            .map(|st| st.volumes.iter().map(|v| v.entries).sum())
            .unwrap_or(0)
    })
}

pub fn service_action_busy() -> bool {
    service_busy_sig().get().0
}

pub fn service_action_label() -> Option<String> {
    service_label_sig().get().0
}

pub fn service_action_error() -> Option<String> {
    service_error_sig().get().0
}

pub fn auto_refresh() -> bool {
    auto_refresh_sig().get().0
}

pub fn set_auto_refresh(enabled: bool) {
    auto_refresh_sig().set(AutoRefreshState(enabled));
    if let Some(engine) = current_engine() {
        engine.lock().set_auto_refresh(enabled);
    }
    persist_prefs();
}

pub fn autostart_enabled() -> bool {
    autostart_sig().get().0
}

pub fn set_autostart_enabled(enabled: bool) {
    if is_test_seeded() {
        autostart_sig().set(AutoStartState(enabled));
        return;
    }
    match bento::autostart::set_autostart_enabled(crate::APP_META.name, enabled) {
        Ok(()) => {
            autostart_sig().set(AutoStartState(enabled));
            notices().notify_ok(if enabled {
                "已开启开机自启"
            } else {
                "已关闭开机自启"
            });
        }
        Err(err) => {
            log::warn!("Failed to update autostart setting: {err}");
            let actual = bento::autostart::is_autostart_enabled(crate::APP_META.name)
                .unwrap_or(!enabled);
            autostart_sig().set(AutoStartState(actual));
            notices().notify_bad(format!("更新开机自启失败：{err}"));
        }
    }
}

// --- Preferences: Theme, CloseBehavior, Global Hotkey ---

pub fn theme_mode() -> ThemeMode {
    theme_mode_sig().get().0
}

pub fn set_theme_mode(mode: ThemeMode) {
    theme_mode_sig().set(ThemeModeState(mode));
    mode.apply();
    persist_prefs();
}

pub fn close_behavior() -> CloseBehavior {
    close_behavior_sig().get().0
}

pub fn close_behavior_untracked() -> CloseBehavior {
    close_behavior_sig().get_untracked().0
}

pub fn set_close_behavior(behavior: CloseBehavior) {
    close_behavior_sig().set(CloseBehaviorState(behavior));
    persist_prefs();
}

pub fn global_hotkey() -> HotkeyBinding {
    global_hotkey_sig().get().0
}

pub fn set_global_hotkey(binding: HotkeyBinding) {
    global_hotkey_sig().set(GlobalHotkeyState(binding));
    persist_prefs();
}

pub fn hotkey_status() -> HotkeyStatus {
    hotkey_status_sig().get().0
}

pub fn set_hotkey_status(status: HotkeyStatus) {
    hotkey_status_sig().set(HotkeyStatusState(status));
}

// --- Notices, Updater, & Modal Dialogs ---

pub fn notices() -> NoticeCenter {
    NoticeCenter::global()
}

pub fn update_status() -> UpdateStatus {
    update_status_sig().get().0
}

pub fn available_update_version() -> Option<String> {
    match update_status_sig().get().0 {
        UpdateStatus::Available(info) => Some(info.version),
        _ => None,
    }
}

pub fn settings_open() -> bool {
    settings_open_sig().get().0
}

pub fn set_settings_open(open: bool) {
    if open && !is_test_seeded() {
        if let Ok(actual) = bento::autostart::is_autostart_enabled(crate::APP_META.name) {
            autostart_sig().set(AutoStartState(actual));
        }
    }
    settings_open_sig().set(SettingsOpen(open));
}

pub fn about_open() -> bool {
    about_open_sig().get().0
}

pub fn set_about_open(open: bool) {
    about_open_sig().set(AboutOpen(open));
}

pub fn syntax_help_open() -> bool {
    syntax_help_open_sig().get().0
}

pub fn set_syntax_help_open(open: bool) {
    syntax_help_open_sig().set(SyntaxHelpOpen(open));
}

pub fn uninstall_confirm_open() -> bool {
    uninstall_confirm_open_sig().get().0
}

pub fn set_uninstall_confirm_open(open: bool) {
    uninstall_confirm_open_sig().set(UninstallConfirmOpen(open));
}

/// Closes whichever modal dialog is topmost (returns `true` if a dialog was closed).
pub fn dismiss_top_dialog() -> bool {
    if uninstall_confirm_open_sig().get_untracked().0 {
        set_uninstall_confirm_open(false);
        return true;
    }
    if syntax_help_open_sig().get_untracked().0 {
        set_syntax_help_open(false);
        return true;
    }
    if settings_open_sig().get_untracked().0 {
        set_settings_open(false);
        return true;
    }
    if about_open_sig().get_untracked().0 {
        set_about_open(false);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramen_prefs_default_and_json_round_trip() {
        let def = RamenPrefs::default();
        assert_eq!(def.theme_mode, ThemeMode::System);
        assert_eq!(def.close_behavior, CloseBehavior::Ask);
        assert!(def.global_hotkey.enabled);
        assert_eq!(def.global_hotkey.keystroke, "alt+space");
        assert!(!def.match_case);
        assert!(def.auto_refresh);

        let custom = RamenPrefs {
            theme_mode: ThemeMode::Dark,
            close_behavior: CloseBehavior::Tray,
            global_hotkey: HotkeyBinding::new(false, "ctrl+shift+space"),
            match_case: true,
            auto_refresh: false,
        };
        let json = serde_json::to_string(&custom).unwrap();
        let back: RamenPrefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back, custom);

        // Partial JSON falls back cleanly for missing fields.
        let partial: RamenPrefs = serde_json::from_str(r#"{"match_case":true}"#).unwrap();
        assert!(partial.match_case);
        assert!(partial.global_hotkey.enabled);
        assert_eq!(partial.global_hotkey.keystroke, "alt+space");
    }

    #[test]
    fn search_row_splits_path_detects_icon_and_formats_metadata() {
        let file = SearchRow::from_item(ResultItem {
            path: r"C:\Projects\Ramen\src\main.rs".to_owned(),
            size: Some(2048),
            mtime_ft: Some(133_500_000_000_000_000),
            is_dir: false,
            ..Default::default()
        });
        assert_eq!(file.file_name.as_str(), "main.rs");
        assert_eq!(file.parent_dir.as_str(), r"C:\Projects\Ramen\src");
        assert_eq!(file.icon_kind, FileIconKind::Code);
        assert_eq!(file.size_label.as_str(), "2.0 KB");
        assert_eq!(file.mtime_label.len(), 16);

        let dir = SearchRow::from_item(ResultItem {
            path: r"C:\Projects\Ramen".to_owned(),
            size: None,
            mtime_ft: None,
            is_dir: true,
            ..Default::default()
        });
        assert_eq!(dir.file_name.as_str(), "Ramen");
        assert_eq!(dir.parent_dir.as_str(), r"C:\Projects");
        assert_eq!(dir.icon_kind, FileIconKind::Folder);
        assert_eq!(dir.size_label.as_str(), "—");
        assert_eq!(dir.mtime_label.as_str(), "—");
    }

    #[test]
    fn daemon_ui_state_tracks_connection_and_version_mismatch() {
        assert!(!DaemonUiState::Connecting.is_connected());
        assert_eq!(DaemonUiState::Connecting.mismatched_server(), None);

        let ok = DaemonUiState::from_connection(DaemonConnectionState::Connected {
            server: "gozd 0.7.1".to_owned(),
            ready: true,
            version_matches: true,
            supports_watch: true,
            features: vec!["watch".to_owned()],
        });
        assert!(ok.is_connected());
        assert_eq!(ok.mismatched_server(), None);

        let old = DaemonUiState::from_connection(DaemonConnectionState::Connected {
            server: "gozd 0.6.0".to_owned(),
            ready: true,
            version_matches: false,
            supports_watch: false,
            features: vec![],
        });
        assert!(old.is_connected());
        assert_eq!(old.mismatched_server(), Some("gozd 0.6.0"));
    }
}
