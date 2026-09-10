use anyhow::Context;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

mod disruptions;
mod goals;
mod migrations;
mod resources;
mod sessions;

#[cfg(test)]
mod tests;

pub struct Database {
    conn: Connection,
    focused_threshold_ms: i64,
    /// Monotonic start instants for sessions started by this daemon process.
    /// Wall-clock time can jump (dual-boot with Windows, NTP corrections), but
    /// `Instant` is monotonic and keeps durations accurate (and never negative).
    monotonic_starts: Mutex<HashMap<i64, Instant>>,
}

#[derive(Debug, Clone)]
pub struct Goal {
    pub id: Option<i64>,
    pub name: String,
    pub target_type: String, // "all" | "class"
    pub target_key: Option<String>,
    pub daily_target_ms: i64,
    pub enabled: bool,
}

impl Database {
    pub fn open(path: &Path, focused_threshold_seconds: u64) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database: {:?}", path))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )?;

        Ok(Self {
            conn,
            focused_threshold_ms: (focused_threshold_seconds * 1000) as i64,
            monotonic_starts: Mutex::new(HashMap::new()),
        })
    }
}
