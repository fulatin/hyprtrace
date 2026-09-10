use anyhow::Context;
use rusqlite::Connection;
use std::path::Path;

mod ai_conversations;
mod apps;
mod categories;
mod efficiency;
mod goals;
mod hourly;
mod insights_sql;
mod like;
mod migrate;
mod predict;
mod projects;
mod report;
mod resources;
mod retention;
mod sessions;
mod status;
mod summaries;
mod workspace;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub struct Database {
    conn: Connection,
    focused_threshold_ms: i64,
}

impl Database {
    pub fn open(path: &Path, focused_threshold_seconds: u64) -> anyhow::Result<Self> {
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
        })
    }
}
