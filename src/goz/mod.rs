//! Backend integration with the `goz` (v0.7.1) NTFS file indexing daemon.

pub mod client;
pub mod query;
pub mod service;
pub mod version;

pub use client::{
    ClientError, DaemonConnectionState, EngineEvent, GozClient, HelloInfo, QueryOutcome,
    SearchEngine, SearchParams,
};
pub use goz_core::proto::{DaemonStatus, ResultItem, VolumeGen, VolumeStatus};
pub use goz_core::types::{SortDir, SortKey, SortSpec, VolumePhase};
pub use query::{FileCategory, FileIconKind, compose_query, detect_file_kind, split_path};
pub use service::{
    DaemonServiceState, install_or_upgrade_service, installed_gozd_path, query_service_state,
    start_service, uninstall_service,
};
pub use version::{
    PINNED_GOZ_TAG, PINNED_GOZ_VERSION, locate_gozd_binary, require_gozd_binary,
    server_matches_pinned,
};
