use rusqlite::params;

impl super::Database {
    /// Record a desktop notification (an interruption).
    pub fn save_notification(&self, app: &str, summary: &str) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        let date_local = now
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();
        self.conn.execute(
            "INSERT INTO disruptions (kind, app, summary, occurred_at, date_local) VALUES ('notification', ?1, ?2, ?3, ?4)",
            params![app, summary, now.to_rfc3339(), date_local],
        )?;
        Ok(())
    }

    /// Record a clipboard copy event.
    pub fn save_clipboard(&self) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        let date_local = now
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();
        self.conn.execute(
            "INSERT INTO disruptions (kind, occurred_at, date_local) VALUES ('clipboard', ?1, ?2)",
            params![now.to_rfc3339(), date_local],
        )?;
        Ok(())
    }
}
