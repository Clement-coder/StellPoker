//! MPC session state caching for coordinator restart recovery.
//!
//! Persists active session state to an embedded SQLite database.
//! On restart, sessions can be replayed from the last checkpoint
//! rather than starting over.

use rusqlite::{params, Connection, Result as SqliteResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub type SessionCacheStore = Arc<Mutex<SessionCache>>;

pub struct SessionCache {
    conn: Connection,
}

#[derive(Clone, Debug)]
pub struct CachedSession {
    pub session_id: String,
    pub table_id: u32,
    pub phase: String,
    pub circuit_name: String,
    pub deck_root: String,
    pub hand_commitments: Vec<String>,
    pub player_order: Vec<String>,
    pub dealt_indices: Vec<u32>,
    pub board_indices: Vec<u32>,
    pub reveal_tx_hashes: HashMap<String, String>,
    pub proof_nonce: u64,
    pub last_checkpoint: i64,
}

impl SessionCache {
    pub fn new(db_path: &str) -> SqliteResult<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "BEGIN; CREATE TABLE IF NOT EXISTS mpc_sessions (
                session_id TEXT PRIMARY KEY,
                table_id INTEGER NOT NULL,
                phase TEXT NOT NULL,
                circuit_name TEXT NOT NULL,
                deck_root TEXT,
                hand_commitments TEXT NOT NULL,
                player_order TEXT NOT NULL,
                dealt_indices TEXT NOT NULL,
                board_indices TEXT NOT NULL,
                reveal_tx_hashes TEXT NOT NULL,
                proof_nonce INTEGER NOT NULL,
                last_checkpoint INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            ); COMMIT;",
        )?;

        Ok(SessionCache { conn })
    }

    pub fn save_session(&self, session: &CachedSession) -> SqliteResult<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let hand_commitments_json =
            serde_json::to_string(&session.hand_commitments).unwrap_or_default();
        let player_order_json = serde_json::to_string(&session.player_order).unwrap_or_default();
        let dealt_indices_json =
            serde_json::to_string(&session.dealt_indices).unwrap_or_default();
        let board_indices_json =
            serde_json::to_string(&session.board_indices).unwrap_or_default();
        let reveal_tx_hashes_json =
            serde_json::to_string(&session.reveal_tx_hashes).unwrap_or_default();

        self.conn.execute(
            "INSERT OR REPLACE INTO mpc_sessions (
                session_id, table_id, phase, circuit_name, deck_root, hand_commitments,
                player_order, dealt_indices, board_indices, reveal_tx_hashes,
                proof_nonce, last_checkpoint, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                &session.session_id,
                session.table_id as i32,
                &session.phase,
                &session.circuit_name,
                &session.deck_root,
                hand_commitments_json,
                player_order_json,
                dealt_indices_json,
                board_indices_json,
                reveal_tx_hashes_json,
                session.proof_nonce as i64,
                session.last_checkpoint,
                now,
                now,
            ],
        )?;

        Ok(())
    }

    pub fn load_session(&self, session_id: &str) -> SqliteResult<Option<CachedSession>> {
        let mut stmt = self
            .conn
            .prepare("SELECT table_id, phase, circuit_name, deck_root, hand_commitments, player_order, dealt_indices, board_indices, reveal_tx_hashes, proof_nonce, last_checkpoint FROM mpc_sessions WHERE session_id = ?")?;

        let result = stmt.query_row(rusqlite::params![session_id], |row| {
            let hand_commitments_json: String = row.get(4)?;
            let player_order_json: String = row.get(5)?;
            let dealt_indices_json: String = row.get(6)?;
            let board_indices_json: String = row.get(7)?;
            let reveal_tx_hashes_json: String = row.get(8)?;

            let hand_commitments: Vec<String> =
                serde_json::from_str(&hand_commitments_json).unwrap_or_default();
            let player_order: Vec<String> =
                serde_json::from_str(&player_order_json).unwrap_or_default();
            let dealt_indices: Vec<u32> =
                serde_json::from_str(&dealt_indices_json).unwrap_or_default();
            let board_indices: Vec<u32> =
                serde_json::from_str(&board_indices_json).unwrap_or_default();
            let reveal_tx_hashes: HashMap<String, String> =
                serde_json::from_str(&reveal_tx_hashes_json).unwrap_or_default();

            Ok(CachedSession {
                session_id: session_id.to_string(),
                table_id: row.get::<_, i32>(0)? as u32,
                phase: row.get(1)?,
                circuit_name: row.get(2)?,
                deck_root: row.get(3)?,
                hand_commitments,
                player_order,
                dealt_indices,
                board_indices,
                reveal_tx_hashes,
                proof_nonce: row.get::<_, i64>(9)? as u64,
                last_checkpoint: row.get(10)?,
            })
        });

        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn load_all_sessions(&self) -> SqliteResult<Vec<CachedSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, table_id, phase, circuit_name, deck_root, hand_commitments, player_order, dealt_indices, board_indices, reveal_tx_hashes, proof_nonce, last_checkpoint FROM mpc_sessions ORDER BY updated_at DESC",
        )?;

        let sessions = stmt
            .query_map([], |row| {
                let hand_commitments_json: String = row.get(5)?;
                let player_order_json: String = row.get(6)?;
                let dealt_indices_json: String = row.get(7)?;
                let board_indices_json: String = row.get(8)?;
                let reveal_tx_hashes_json: String = row.get(9)?;

                let hand_commitments: Vec<String> =
                    serde_json::from_str(&hand_commitments_json).unwrap_or_default();
                let player_order: Vec<String> =
                    serde_json::from_str(&player_order_json).unwrap_or_default();
                let dealt_indices: Vec<u32> =
                    serde_json::from_str(&dealt_indices_json).unwrap_or_default();
                let board_indices: Vec<u32> =
                    serde_json::from_str(&board_indices_json).unwrap_or_default();
                let reveal_tx_hashes: HashMap<String, String> =
                    serde_json::from_str(&reveal_tx_hashes_json).unwrap_or_default();

                Ok(CachedSession {
                    session_id: row.get(0)?,
                    table_id: row.get::<_, i32>(1)? as u32,
                    phase: row.get(2)?,
                    circuit_name: row.get(3)?,
                    deck_root: row.get(4)?,
                    hand_commitments,
                    player_order,
                    dealt_indices,
                    board_indices,
                    reveal_tx_hashes,
                    proof_nonce: row.get::<_, i64>(10)? as u64,
                    last_checkpoint: row.get(11)?,
                })
            })?
            .collect::<SqliteResult<Vec<_>>>()?;

        Ok(sessions)
    }

    pub fn delete_session(&self, session_id: &str) -> SqliteResult<()> {
        self.conn
            .execute("DELETE FROM mpc_sessions WHERE session_id = ?", params![session_id])?;
        Ok(())
    }

    pub fn cleanup_old_sessions(&self, before_timestamp: i64) -> SqliteResult<usize> {
        self.conn.execute(
            "DELETE FROM mpc_sessions WHERE updated_at < ?",
            params![before_timestamp],
        )
    }
}

pub fn new_store(db_path: &str) -> SessionCacheStore {
    match SessionCache::new(db_path) {
        Ok(cache) => Arc::new(Mutex::new(cache)),
        Err(e) => {
            tracing::error!("Failed to initialize session cache: {}", e);
            Arc::new(Mutex::new(
                SessionCache::new(":memory:").expect("in-memory fallback"),
            ))
        }
    }
}
