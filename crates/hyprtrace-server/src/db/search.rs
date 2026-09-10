use crate::models::{SearchHit, Session};
use rusqlite::params;

/// Longest query string accepted, and the most terms kept from it. FTS5 will
/// happily try to satisfy `a AND b AND c AND ...` for a thousand terms; the cap
/// keeps one request from turning into a huge expression tree.
const MAX_QUERY_CHARS: usize = 200;
const MAX_TERMS: usize = 8;

/// The trigram tokenizer indexes three-character sequences, so a shorter query
/// cannot match through `MATCH` at all — it returns an empty result rather than
/// an error, which is the worst kind of failure. A two-character Chinese word
/// (`课表`) is an entirely ordinary query, so short input goes through `LIKE`
/// instead: without the index a few tens of thousands of titles is a scan
/// nobody notices.
const MIN_TRIGRAM_CHARS: usize = 3;

/// Turn whatever the user typed into an FTS5 MATCH expression.
///
/// The raw string cannot be passed through: FTS5 has its own query language, so
/// a search for `foo -bar` or a stray quote is a syntax ERROR rather than a
/// search that finds nothing, and the endpoint would answer 500 to ordinary
/// input. Every word is quoted as a phrase (embedded quotes doubled to escape
/// them) and the phrases are ANDed, which is what typing several words means.
///
/// Returns `None` when nothing searchable is left.
fn fts_match_expression(raw: &str) -> Option<String> {
    let terms: Vec<String> = raw
        .split_whitespace()
        .take(MAX_TERMS)
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

/// `LIKE` pattern for a literal substring match, with the wildcards escaped.
fn like_pattern(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 2);
    pattern.push('%');
    for ch in query.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(ch);
    }
    pattern.push('%');
    pattern
}

impl super::Database {
    /// Create the full-text index over window titles, and keep it in sync.
    ///
    /// The index is owned by the server (like `app_categories` and `projects`)
    /// rather than declared in `schema/hyprtrace.sql`: it is derived data, the
    /// daemon has no business knowing about it, and the TRIGGERS below live in
    /// the database rather than in the code — so the daemon's inserts and the
    /// delete paths keep the index current without either component mentioning
    /// it.
    ///
    /// A SQLite build without FTS5 is not fatal: the index is skipped, a warning
    /// is logged, and [`Self::search_sessions`] returns no hits.
    pub fn ensure_search_index(&self) -> anyhow::Result<()> {
        let existed: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'sessions_fts'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);

        let ddl = "CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
                       title,
                       content='sessions',
                       content_rowid='id',
                       tokenize='trigram'
                   );
                   CREATE TRIGGER IF NOT EXISTS sessions_fts_ai AFTER INSERT ON sessions BEGIN
                       INSERT INTO sessions_fts(rowid, title) VALUES (new.id, new.title);
                   END;
                   CREATE TRIGGER IF NOT EXISTS sessions_fts_ad AFTER DELETE ON sessions BEGIN
                       INSERT INTO sessions_fts(sessions_fts, rowid, title)
                       VALUES ('delete', old.id, old.title);
                   END;
                   CREATE TRIGGER IF NOT EXISTS sessions_fts_au AFTER UPDATE ON sessions BEGIN
                       INSERT INTO sessions_fts(sessions_fts, rowid, title)
                       VALUES ('delete', old.id, old.title);
                       INSERT INTO sessions_fts(rowid, title) VALUES (new.id, new.title);
                   END;";

        if let Err(e) = self.conn.execute_batch(ddl) {
            log::warn!("Full-text search is disabled: this SQLite build has no FTS5 ({e})");
            return Ok(());
        }

        // An external-content FTS5 table starts empty: without this one-off
        // rebuild, every session recorded before the index existed would be
        // invisible to search. It runs only when the table is first created, so
        // a restart does not re-index the whole history.
        if !existed {
            let total: i64 = self
                .conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
                .unwrap_or(0);
            if total > 0 {
                self.conn
                    .execute("INSERT INTO sessions_fts(sessions_fts) VALUES('rebuild')", [])?;
                log::info!("Built the window-title search index over {total} session(s)");
            }
        }
        Ok(())
    }

    /// Full-text search over window titles.
    ///
    /// `from`/`to` are optional local dates; an empty string means "no bound".
    /// `limit` is clamped here as well as in the handler.
    pub fn search_sessions(
        &self,
        query: &str,
        from: &str,
        to: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<SearchHit>> {
        let query: String = query.chars().take(MAX_QUERY_CHARS).collect();
        let query = query.trim();
        // Clamp in the query, not only in the handler: a negative LIMIT reaches
        // SQLite as "no limit at all".
        let limit = limit.clamp(1, 200);
        if query.is_empty() {
            return Ok(Vec::new());
        }

        if query.chars().count() >= MIN_TRIGRAM_CHARS {
            match fts_match_expression(query) {
                Some(expression) => self.search_by_index(&expression, from, to, limit),
                None => Ok(Vec::new()),
            }
        } else {
            self.search_by_substring(query, from, to, limit)
        }
    }

    fn search_by_index(
        &self,
        match_expression: &str,
        from: &str,
        to: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<SearchHit>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.class, s.title, s.workspace, s.started_at, s.ended_at,
                    s.duration_ms, s.activity_state, s.focused_ms,
                    snippet(sessions_fts, 0, '', '', ' … ', 12)
             FROM sessions_fts
             JOIN sessions s ON s.id = sessions_fts.rowid
             WHERE sessions_fts MATCH ?1
               AND (?2 = '' OR s.date_local >= ?2)
               AND (?3 = '' OR s.date_local <= ?3)
             ORDER BY bm25(sessions_fts), s.started_at DESC
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(params![match_expression, from, to, limit as i64], |row| {
            Ok(SearchHit {
                session: Session {
                    id: row.get(0)?,
                    class: row.get(1)?,
                    title: row.get(2)?,
                    workspace: row.get(3)?,
                    started_at: row.get(4)?,
                    ended_at: row.get(5)?,
                    duration_ms: row.get(6)?,
                    activity_state: row.get(7)?,
                    focused_ms: row.get(8)?,
                },
                snippet: row.get(9)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// The path for queries too short for the trigram index. Newest first, since
    /// there is no relevance ranking to sort by.
    fn search_by_substring(
        &self,
        query: &str,
        from: &str,
        to: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<SearchHit>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.class, s.title, s.workspace, s.started_at, s.ended_at,
                    s.duration_ms, s.activity_state, s.focused_ms
             FROM sessions s
             WHERE s.title LIKE ?1 ESCAPE '\\'
               AND (?2 = '' OR s.date_local >= ?2)
               AND (?3 = '' OR s.date_local <= ?3)
             ORDER BY s.started_at DESC
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![like_pattern(query), from, to, limit as i64],
            |row| {
                Ok(SearchHit {
                    session: Session {
                        id: row.get(0)?,
                        class: row.get(1)?,
                        title: row.get(2)?,
                        workspace: row.get(3)?,
                        started_at: row.get(4)?,
                        ended_at: row.get(5)?,
                        duration_ms: row.get(6)?,
                        activity_state: row.get(7)?,
                        focused_ms: row.get(8)?,
                    },
                    // The title is short, so it is its own excerpt here.
                    snippet: row.get(2)?,
                })
            },
        )?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::test_db;

    fn seed(db: &crate::db::Database) {
        for (title, date) in [
            ("main.rs — hyprtrace", "2025-03-04"),
            ("Applied Energistics 2 pattern matrix", "2025-03-05"),
            ("班级课表 — Firefox", "2025-03-06"),
        ] {
            db.conn
                .execute(
                    "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, date_local)
                     VALUES ('kitty', ?1, ?2 || 'T10:00:00Z', ?2 || 'T10:05:00Z', 300000, ?2)",
                    params![title, date],
                )
                .unwrap();
        }
    }

    /// A search is a SUBSTRING search, which is what "where did I see this"
    /// means: `hypr` has to find `hyprtrace`, and `main.rs` has to match the
    /// title even though a word tokenizer would split it.
    #[test]
    fn search_matches_inside_a_title() {
        let db = test_db();
        db.ensure_search_index().unwrap();
        seed(&db);

        for query in ["hyprtrace", "hypr", "main.rs"] {
            let hits = db.search_sessions(query, "", "", 50).unwrap();
            assert_eq!(hits.len(), 1, "query {query:?} should find the title");
            assert_eq!(hits[0].session.title, "main.rs — hyprtrace");
        }

        // Several words are ANDed, order-independent and case-insensitive.
        let hits = db.search_sessions("MATRIX pattern", "", "", 50).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("pattern"));
    }

    /// A two-character Chinese word is an ordinary query, and it is BELOW the
    /// trigram tokenizer's three-character minimum — through MATCH alone it
    /// would silently return nothing, so short queries take the LIKE path.
    #[test]
    fn short_queries_still_match() {
        let db = test_db();
        db.ensure_search_index().unwrap();
        seed(&db);

        let hits = db.search_sessions("课表", "", "", 50).unwrap();
        assert_eq!(hits.len(), 1, "a CJK two-character query must match");
        assert_eq!(hits[0].session.title, "班级课表 — Firefox");

        // One ASCII character is the same situation.
        let hits = db.search_sessions("class", "", "", 50).unwrap();
        assert!(hits.is_empty(), "that word is in no title");
        let hits = db.search_sessions("e", "", "", 50).unwrap();
        assert_eq!(hits.len(), 3, "a single letter is a substring of all three");
    }

    /// `%` and `_` are LIKE wildcards and must be literal in a search.
    #[test]
    fn like_wildcards_are_literal() {
        let db = test_db();
        db.ensure_search_index().unwrap();
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, date_local)
                 VALUES ('kitty', '100% done', '2025-03-04T10:00:00Z', '2025-03-04T10:05:00Z', 300000, '2025-03-04')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, date_local)
                 VALUES ('kitty', '1000 done', '2025-03-04T11:00:00Z', '2025-03-04T11:05:00Z', 300000, '2025-03-04')",
                [],
            )
            .unwrap();

        // `%%` must not match everything: the pattern is escaped.
        let hits = db.search_sessions("%%", "", "", 50).unwrap();
        assert!(hits.is_empty(), "an escaped wildcard matches nothing here");
        let hits = db.search_sessions("0%", "", "", 50).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session.title, "100% done");
    }

    /// The index has to follow the table, including rows the daemon writes and
    /// rows the delete paths remove — the triggers are what make that true.
    #[test]
    fn the_index_follows_inserts_updates_and_deletes() {
        let db = test_db();
        db.ensure_search_index().unwrap();

        db.conn
            .execute(
                "INSERT INTO sessions (class, title, started_at, ended_at, duration_ms, date_local)
                 VALUES ('kitty', 'before the change', '2025-03-04T10:00:00Z', '2025-03-04T10:05:00Z', 300000, '2025-03-04')",
                [],
            )
            .unwrap();
        let id = db.conn.last_insert_rowid();
        assert_eq!(db.search_sessions("change", "", "", 50).unwrap().len(), 1);

        db.conn
            .execute(
                "UPDATE sessions SET title = 'after the rewrite' WHERE id = ?1",
                params![id],
            )
            .unwrap();
        assert_eq!(
            db.search_sessions("rewrite", "", "", 50).unwrap().len(),
            1,
            "an UPDATE must reach the index"
        );
        assert_eq!(
            db.search_sessions("change", "", "", 50).unwrap().len(),
            0,
            "the old text must be gone from the index"
        );

        db.conn
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])
            .unwrap();
        assert_eq!(
            db.search_sessions("rewrite", "", "", 50).unwrap().len(),
            0,
            "a DELETE must reach the index"
        );
    }

    /// FTS5 has its own query language and `LIKE` has its own wildcards: no
    /// input may turn a search into a failed request.
    #[test]
    fn awkward_input_is_a_search_not_an_error() {
        let db = test_db();
        db.ensure_search_index().unwrap();
        seed(&db);

        for query in [
            "\"", "foo -bar", "a AND b", "*", "()", "^", "hyprtrace OR", "NEAR(", "''",
            "\"unbalanced", "%", "_",
        ] {
            let hits = db
                .search_sessions(query, "", "", 50)
                .unwrap_or_else(|e| panic!("query {query:?} must not fail: {e}"));
            assert!(hits.len() <= 3, "query {query:?} returned nonsense");
        }

        assert!(
            db.search_sessions("   ", "", "", 50).unwrap().is_empty(),
            "a blank query has nothing to search for"
        );
    }

    #[test]
    fn the_date_range_and_the_limit_bound_the_hits() {
        let db = test_db();
        db.ensure_search_index().unwrap();
        seed(&db);

        assert_eq!(db.search_sessions("e", "", "", 50).unwrap().len(), 3);
        let bounded = db.search_sessions("e", "2025-03-05", "2025-03-05", 50).unwrap();
        assert_eq!(bounded.len(), 1);
        assert_eq!(
            bounded[0].session.title,
            "Applied Energistics 2 pattern matrix"
        );

        assert_eq!(db.search_sessions("e", "", "", 2).unwrap().len(), 2);
        // A limit of 0 would ask SQLite for nothing; it clamps up to 1.
        assert_eq!(db.search_sessions("e", "", "", 0).unwrap().len(), 1);
    }
}
