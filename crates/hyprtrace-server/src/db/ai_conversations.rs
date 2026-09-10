use crate::models::AiMessage;
use rusqlite::params;

impl super::Database {
    pub fn save_ai_message(&self, role: &str, content: &str, model: &str) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO ai_conversations (created_at, role, content, model) VALUES (?1, ?2, ?3, ?4)",
            params![now, role, content, model],
        )?;
        Ok(())
    }

    pub fn save_ai_message_streaming(
        &self,
        role: &str,
        content: &str,
        model: &str,
    ) -> anyhow::Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO ai_conversations (created_at, role, content, model, complete) VALUES (?1, ?2, ?3, ?4, 0)",
            params![now, role, content, model],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_ai_message(&self, id: i64, content: &str, complete: bool) -> anyhow::Result<()> {
        self.conn.execute(
            "UPDATE ai_conversations SET content = ?1, complete = ?2 WHERE id = ?3",
            params![content, if complete { 1 } else { 0 }, id],
        )?;
        Ok(())
    }

    pub fn ai_conversations(&self, limit: usize) -> anyhow::Result<Vec<AiMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, role, content, model, complete FROM ai_conversations
             ORDER BY created_at DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            let complete: Option<i64> = row.get(5)?;
            Ok(AiMessage {
                id: row.get(0)?,
                created_at: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                model: row.get(4)?,
                complete: complete.map(|v| v != 0),
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        results.reverse();
        Ok(results)
    }

    pub fn clear_ai_conversations(&self) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM ai_conversations", [])?;
        Ok(())
    }
}
