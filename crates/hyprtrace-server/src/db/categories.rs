use crate::models::CategoryRule;
use rusqlite::params;

use super::like::like_match;

impl super::Database {
    /// Create the app_categories table (if missing) and seed default rules.
    pub fn ensure_categories(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_categories (
                id       INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern  TEXT NOT NULL UNIQUE,
                category TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 0
            );",
        )?;

        let defaults: &[(&str, &str)] = &[
            ("code", "development"),
            ("code-insiders", "development"),
            ("codium", "development"),
            ("kitty", "development"),
            ("alacritty", "development"),
            ("wezterm", "development"),
            ("ghostty", "development"),
            ("foot", "development"),
            ("vim", "development"),
            ("nvim", "development"),
            ("emacs", "development"),
            ("idea%", "development"),
            ("pycharm%", "development"),
            ("webstorm%", "development"),
            ("goland%", "development"),
            ("clion%", "development"),
            ("firefox", "browsing"),
            ("chromium", "browsing"),
            ("google-chrome%", "browsing"),
            ("microsoft-edge%", "browsing"),
            ("brave-browser", "browsing"),
            ("zen", "browsing"),
            ("minecraft%", "gaming"),
            ("steam%", "gaming"),
            ("lutris", "gaming"),
            ("heroic", "gaming"),
            ("qq%", "social"),
            ("wechat%", "social"),
            ("telegram%", "social"),
            ("discord", "social"),
            ("mihomo%", "utility"),
            ("kdesystemsettings", "system"),
            ("systemsettings", "system"),
            ("gnome-control-center", "system"),
            ("dolphin", "system"),
            ("nautilus", "system"),
            ("vlc", "media"),
            ("mpv", "media"),
            ("kdenlive", "media"),
            ("obs", "media"),
            ("spotify", "media"),
            ("zathura", "productivity"),
            ("obsidian", "productivity"),
            ("logseq", "productivity"),
            ("thunderbird", "productivity"),
            ("evolution", "productivity"),
        ];
        let mut stmt = self.conn.prepare(
            "INSERT OR IGNORE INTO app_categories (pattern, category, priority) VALUES (?1, ?2, 0)",
        )?;
        for (pattern, category) in defaults {
            stmt.execute(params![pattern, category])?;
        }
        Ok(())
    }
    /// All category rules, highest priority first.
    pub fn categories(&self) -> anyhow::Result<Vec<CategoryRule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, pattern, category, priority FROM app_categories
             ORDER BY priority DESC, id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CategoryRule {
                id: Some(row.get(0)?),
                pattern: row.get(1)?,
                category: row.get(2)?,
                priority: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Replace all category rules with the given list.
    pub fn set_categories(&self, rules: &[CategoryRule]) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM app_categories", [])?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO app_categories (pattern, category, priority) VALUES (?1, ?2, ?3)",
        )?;
        for r in rules {
            stmt.execute(params![r.pattern.trim(), r.category.trim(), r.priority])?;
        }
        Ok(())
    }

    /// Classify an app class by matching rules (SQLite LIKE semantics, case-insensitive).
    pub fn categorize(&self, class: &str) -> String {
        let rules = match self.categories() {
            Ok(r) => r,
            Err(_) => return "other".to_string(),
        };
        for rule in rules {
            if like_match(&rule.pattern, class) {
                return rule.category;
            }
        }
        "other".to_string()
    }

    pub fn known_categories() -> Vec<String> {
        [
            "development",
            "browsing",
            "gaming",
            "social",
            "media",
            "productivity",
            "utility",
            "system",
            "other",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }
}
