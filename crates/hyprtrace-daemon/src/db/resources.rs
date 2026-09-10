use rusqlite::params;

impl super::Database {
    /// Current open session's (id, class, pid), for resource sampling.
    pub fn current_session_resources(&self) -> anyhow::Result<Option<(i64, String, i32)>> {
        let result = self.conn.query_row(
            "SELECT id, class, pid FROM sessions
             WHERE ended_at IS NULL AND pid IS NOT NULL
             ORDER BY started_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Record a CPU/memory sample for a session.
    pub fn save_resource_sample(
        &self,
        session_id: i64,
        class: &str,
        cpu_pct: f64,
        mem_kb: i64,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        let date_local = now
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();
        self.conn.execute(
            "INSERT INTO app_resources (session_id, class, sampled_at, cpu_pct, mem_kb, date_local)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session_id,
                class,
                now.to_rfc3339(),
                cpu_pct,
                mem_kb,
                date_local
            ],
        )?;
        Ok(())
    }
}
