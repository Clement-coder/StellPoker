use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

const DEFAULT_ARCHIVE_TTL_SECS: u64 = 3600;
const DEFAULT_PURGE_TTL_SECS: u64 = 86400;
const ARCHIVE_INTERVAL_SECS: u64 = 600;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchivedSession {
    pub archive_id: String,
    pub session_id: String,
    pub table_id: u32,
    pub status: String,
    pub cancel_reason: Option<String>,
    pub started_at: String,
    pub archived_at: String,
    pub table_snapshot: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveIndex {
    pub sessions: Vec<ArchivedSession>,
}

#[derive(Clone)]
pub struct ArchiveConfig {
    pub archive_ttl: Duration,
    pub purge_ttl: Duration,
    pub storage_path: PathBuf,
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            archive_ttl: Duration::from_secs(DEFAULT_ARCHIVE_TTL_SECS),
            purge_ttl: Duration::from_secs(DEFAULT_PURGE_TTL_SECS),
            storage_path: PathBuf::from(".tmp/session-archives"),
        }
    }
}

impl ArchiveConfig {
    pub fn from_env() -> Self {
        let archive_ttl = std::env::var("SESSION_ARCHIVE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_ARCHIVE_TTL_SECS);

        let purge_ttl = std::env::var("SESSION_PURGE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_PURGE_TTL_SECS);

        let storage_path = std::env::var("SESSION_ARCHIVE_PATH")
            .unwrap_or_else(|_| ".tmp/session-archives".to_string());

        Self {
            archive_ttl: Duration::from_secs(archive_ttl),
            purge_ttl: Duration::from_secs(purge_ttl),
            storage_path: PathBuf::from(storage_path),
        }
    }
}

pub type ArchiveStore = Arc<RwLock<ArchiveIndex>>;

pub fn new_store() -> ArchiveStore {
    Arc::new(RwLock::new(ArchiveIndex {
        sessions: Vec::new(),
    }))
}

pub async fn archive_session(
    store: &ArchiveStore,
    config: &ArchiveConfig,
    session: &crate::session_gc::MpcSession,
    table_snapshot: Option<String>,
) {
    let archived = ArchivedSession {
        archive_id: uuid::Uuid::new_v4().to_string(),
        session_id: session.session_id.clone(),
        table_id: session.table_id,
        status: session.status.to_string(),
        cancel_reason: session.cancel_reason.clone(),
        started_at: format!("{:?}", session.started_at),
        archived_at: Utc::now().to_rfc3339(),
        table_snapshot,
    };

    {
        let mut idx = store.write().await;
        idx.sessions.push(archived.clone());
    }

    let file_path = config.storage_path.join(format!(
        "{}.json",
        archived.archive_id
    ));

    if let Some(parent) = file_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            tracing::warn!(
                err = %e,
                "failed to create archive directory"
            );
            return;
        }
    }

    match serde_json::to_string_pretty(&archived) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&file_path, &json).await {
                tracing::warn!(
                    archive_id = %archived.archive_id,
                    err = %e,
                    "failed to write archive file"
                );
            } else {
                tracing::info!(
                    archive_id = %archived.archive_id,
                    session_id = %archived.session_id,
                    path = %file_path.display(),
                    "session archived successfully"
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                err = %e,
                "failed to serialize archived session"
            );
        }
    }
}

pub async fn query_archives(
    store: &ArchiveStore,
    session_id: Option<&str>,
    table_id: Option<u32>,
    from_timestamp: Option<DateTime<Utc>>,
    to_timestamp: Option<DateTime<Utc>>,
    limit: usize,
    offset: usize,
) -> Vec<ArchivedSession> {
    let idx = store.read().await;
    let mut results: Vec<ArchivedSession> = idx
        .sessions
        .iter()
        .filter(|s| {
            if let Some(sid) = session_id {
                if s.session_id != sid {
                    return false;
                }
            }
            if let Some(tid) = table_id {
                if s.table_id != tid {
                    return false;
                }
            }
            if let Some(ref from) = from_timestamp {
                if let Ok(ts) = DateTime::parse_from_rfc3339(&s.archived_at) {
                    if ts.with_timezone(&Utc) < *from {
                        return false;
                    }
                }
            }
            if let Some(ref to) = to_timestamp {
                if let Ok(ts) = DateTime::parse_from_rfc3339(&s.archived_at) {
                    if ts.with_timezone(&Utc) > *to {
                        return false;
                    }
                }
            }
            true
        })
        .cloned()
        .collect();

    results.sort_by(|a, b| b.archived_at.cmp(&a.archived_at));
    results.into_iter().skip(offset).take(limit).collect()
}

pub async fn purge_old_archives(
    store: &ArchiveStore,
    config: &ArchiveConfig,
) -> usize {
    let cutoff = Utc::now() - chrono::Duration::from_std(config.purge_ttl).unwrap_or_default();
    let mut idx = store.write().await;

    let mut to_remove = Vec::new();
    idx.sessions.retain(|s| {
        let keep = match DateTime::parse_from_rfc3339(&s.archived_at) {
            Ok(ts) => ts.with_timezone(&Utc) > cutoff,
            Err(_) => true,
        };
        if !keep {
            to_remove.push(s.archive_id.clone());
        }
        keep
    });

    for archive_id in &to_remove {
        let file_path = config.storage_path.join(format!("{}.json", archive_id));
        if file_path.exists() {
            if let Err(e) = tokio::fs::remove_file(&file_path).await {
                tracing::warn!(
                    archive_id = %archive_id,
                    err = %e,
                    "failed to remove archive file during purge"
                );
            }
        }
    }

    let count = to_remove.len();
    if count > 0 {
        tracing::info!(count = count, purge_ttl_secs = ?config.purge_ttl, "purged old session archives");
    }
    count
}

pub async fn load_existing_archives(store: &ArchiveStore, config: &ArchiveConfig) {
    if !config.storage_path.exists() {
        return;
    }

    let mut read_dir = match tokio::fs::read_dir(&config.storage_path).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(err = %e, "failed to read archive directory");
            return;
        }
    };

    let mut loaded = 0u32;
    let mut idx = store.write().await;

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "json") {
            continue;
        }

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                match serde_json::from_str::<ArchivedSession>(&content) {
                    Ok(archived) => {
                        idx.sessions.push(archived);
                        loaded += 1;
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            err = %e,
                            "failed to parse archive file"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    err = %e,
                    "failed to read archive file"
                );
            }
        }
    }

    if loaded > 0 {
        tracing::info!(count = loaded, "loaded existing session archives from disk");
    }
}

pub fn spawn_archive_task(
    mpc_sessions: crate::session_gc::SessionStore,
    tables: Arc<RwLock<HashMap<u32, crate::TableSession>>>,
    archive_store: ArchiveStore,
    config: ArchiveConfig,
) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(ARCHIVE_INTERVAL_SECS);
        loop {
            tokio::time::sleep(interval).await;

            let stale_ids: Vec<(String, crate::session_gc::MpcSession)> = {
                let sessions = mpc_sessions.read().await;
                sessions
                    .values()
                    .filter(|s| {
                        s.status != crate::session_gc::SessionStatus::Running
                            && s.started_at.elapsed() > config.archive_ttl
                    })
                    .map(|s| (s.session_id.clone(), s.clone()))
                    .collect()
            };

            for (session_id, session) in &stale_ids {
                let table_snapshot = {
                    let tables = tables.read().await;
                    tables
                        .get(&session.table_id)
                        .map(|t| serde_json::to_string(t).unwrap_or_default())
                };

                archive_session(&archive_store, &config, session, table_snapshot).await;

                let mut sessions = mpc_sessions.write().await;
                sessions.remove(session_id);
            }

            purge_old_archives(&archive_store, &config).await;
        }
    });
}

pub async fn restore_archived_session(
    store: &ArchiveStore,
    archive_id: &str,
) -> Option<ArchivedSession> {
    let idx = store.read().await;
    idx.sessions
        .iter()
        .find(|s| s.archive_id == archive_id)
        .cloned()
}
