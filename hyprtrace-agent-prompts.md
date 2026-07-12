# HyprTrace — 完整 Agent 开发提示词

以下是一套可直接交付给 AI coding agent（如 Claude Code、Codex CLI、Hermes subagent）的完整开发提示词。
按 Phase 顺序执行，每个 Phase 可作为一个独立任务分派。

---

## 主提示词（Master Prompt）

将此提示词作为系统上下文，让 agent 理解整个项目的目标和约束：

```
你正在开发 HyprTrace，一个 Hyprland 窗口管理器的使用时间追踪系统。
请严格遵循以下约束：

【项目目标】
构建三个组件：
1. hyprtrace-daemon: Rust 守护进程，通过 hyprland-rs 0.4.0-beta.3 监听 Hyprland IPC 事件，
   持续追踪用户正在使用的窗口（class + title + workspace）和时长，写入 SQLite 数据库。
2. hyprtrace-server: Rust Axum HTTP API 服务器，从 SQLite 读取数据，提供 REST API 给前端，
   并集成 AI 分析（支持 Ollama 本地模型和 OpenAI 云端 API，通过配置切换）。
3. hyprtrace-web: React + TypeScript + Tailwind CSS 前端，提供仪表盘、应用排行、时间线、
   AI 对话面板等页面。

【技术约束】
- Rust workspace: Cargo.toml 在根目录，crates/ 下放 hyprtrace-daemon 和 hyprtrace-server
- 前端: web/ 目录，Vite + React 18 + TypeScript，Tailwind CSS 3，Recharts，Lucide Icons
- 数据库: SQLite，路径 ~/.local/share/hyprtrace/hyprtrace.db
- 配置文件: ~/.config/hyprtrace/config.toml
- hyprland-rs 版本锁定 0.4.0-beta.3
- 不使用任何需要 GPU 的依赖
- 所有依赖必须能在 crates.io 或 npm 上获取

【架构约束】
- daemon 和 server 是独立二进制，不通过 IPC 通信，仅共享 SQLite 数据库
- daemon 通过 hyprland-rs EventListener 的 add_active_window_changed_handler 监听窗口切换
- 每个窗口聚焦 → 开始 session，窗口失焦 → 结束 session 并记录时长
- server 提供 CORS-allowed HTTP API（默认 127.0.0.1:9420）
- 前端通过 fetch 调用 server API
- AI 模块抽象为 trait AiProvider，实现 OllamaProvider 和 OpenAiProvider

【代码风格】
- Rust: 使用 anyhow 处理错误，log + env_logger 记录日志，所有 public API 加文档注释
- TypeScript: 严格模式，所有 API 返回类型显式定义
- 命名: snake_case (Rust), camelCase (TypeScript)
- 每个 crate 至少有 1 个集成测试

【实现顺序】
Phase 1 → Phase 2 → Phase 3 → Phase 4，不可跳过。

请按照下面的 Phase 提示词逐一实现。每个 Phase 结束后运行 cargo check / npm run build 确保编译通过。
```

---

## Phase 1 提示词：项目骨架 + 守护进程

```
【Phase 1: 项目初始化和守护进程实现】

请按顺序完成以下任务。每完成一个任务，运行编译检查。

### Task 1.1: 创建 Rust workspace

在 /root/hyprtrace/ 下创建 Cargo workspace：

1. 创建目录结构：
   /root/hyprtrace/Cargo.toml
   /root/hyprtrace/crates/hyprtrace-daemon/Cargo.toml
   /root/hyprtrace/crates/hyprtrace-daemon/src/main.rs
   /root/hyprtrace/crates/hyprtrace-server/Cargo.toml
   /root/hyprtrace/crates/hyprtrace-server/src/main.rs

2. 根 Cargo.toml:
```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
```

3. crates/hyprtrace-daemon/Cargo.toml 依赖:
   - hyprland = "0.4.0-beta.3"
   - rusqlite = { version = "0.31", features = ["bundled"] }
   - serde = { version = "1", features = ["derive"] }
   - serde_json = "1"
   - chrono = { version = "0.4", features = ["serde"] }
   - directories = "5"
   - toml = "0.8"
   - log = "0.4"
   - env_logger = "0.11"
   - anyhow = "1"

4. crates/hyprtrace-server/Cargo.toml 依赖:
   - axum = "0.7"
   - tokio = { version = "1", features = ["full"] }
   - serde = { version = "1", features = ["derive"] }
   - serde_json = "1"
   - rusqlite = { version = "0.31", features = ["bundled"] }
   - chrono = { version = "0.4", features = ["serde"] }
   - directories = "5"
   - toml = "0.8"
   - tower-http = { version = "0.5", features = ["cors"] }
   - reqwest = { version = "0.12", features = ["json"] }
   - anyhow = "1"
   - log = "0.4"
   - env_logger = "0.11"
   - uuid = { version = "1", features = ["v4"] }
   - async-trait = "0.1"

5. 验证: cargo check (从 /root/hyprtrace/)

### Task 1.2: 实现配置模块

创建 crates/hyprtrace-daemon/src/config.rs，包含 Config 结构体和 load() 方法：

```rust
use anyhow::Context;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub ai: AiConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DaemonConfig {
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_seconds: u64,
}

fn default_db_path() -> String {
    "~/.local/share/hyprtrace/hyprtrace.db".to_string()
}

fn default_idle_timeout() -> u64 { 300 }

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String { "127.0.0.1".to_string() }
fn default_port() -> u16 { 9420 }

#[derive(Debug, Deserialize, Clone, Default)]
pub struct AiConfig {
    #[serde(default = "default_ai_provider")]
    pub default_provider: String,
    #[serde(default)]
    pub ollama: OllamaConfig,
    #[serde(default)]
    pub openai: OpenAiConfig,
}

fn default_ai_provider() -> String { "ollama".to_string() }

#[derive(Debug, Deserialize, Clone)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_url")]
    pub base_url: String,
    #[serde(default = "default_ollama_model")]
    pub default_model: String,
}

fn default_ollama_url() -> String { "http://localhost:11434".to_string() }
fn default_ollama_model() -> String { "qwen2.5:7b".to_string() }

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: default_ollama_url(),
            default_model: default_ollama_model(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenAiConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_openai_url")]
    pub base_url: String,
    #[serde(default = "default_openai_model")]
    pub default_model: String,
}

fn default_openai_url() -> String { "https://api.openai.com/v1".to_string() }
fn default_openai_model() -> String { "gpt-4o-mini".to_string() }

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: default_openai_url(),
            default_model: default_openai_model(),
        }
    }
}

impl Config {
    /// 加载配置：先尝试 ~/.config/hyprtrace/config.toml，不存在则用默认值创建
    pub fn load() -> anyhow::Result<Self> {
        let config_dir = dirs_next_config_dir()?;
        let config_path = config_dir.join("config.toml");
        
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("无法读取配置文件: {:?}", config_path))?;
            toml::from_str(&content).context("配置文件格式错误")
        } else {
            let config = Config::default();
            config.save_to(&config_path)?;
            Ok(config)
        }
    }

    fn default() -> Self {
        Self {
            daemon: DaemonConfig {
                db_path: default_db_path(),
                idle_timeout_seconds: default_idle_timeout(),
            },
            server: ServerConfig::default(),
            ai: AiConfig::default(),
        }
    }

    fn save_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self).context("序列化配置失败")?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }

    /// 展开 db_path 中的 ~ 为用户 home 目录
    pub fn db_path_expanded(&self) -> PathBuf {
        expand_tilde(&self.daemon.db_path)
    }

    /// 确保数据库目录存在
    pub fn ensure_db_dir(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.db_path_expanded().parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

fn dirs_next_config_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    Ok(PathBuf::from(home).join(".config/hyprtrace"))
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        PathBuf::from(path.replacen('~', &home, 1))
    } else {
        PathBuf::from(path)
    }
}
```

验证: cargo check -p hyprtrace-daemon

### Task 1.3: 实现数据库模块

创建 crates/hyprtrace-daemon/src/db.rs：

```rust
use anyhow::Context;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    /// 打开 SQLite 数据库，启用 WAL 模式和 busy_timeout
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("无法打开数据库: {:?}", path))?;
        
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;"
        )?;
        
        Ok(Self { conn })
    }

    /// 创建表结构
    pub fn migrate(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                class       TEXT NOT NULL,
                title       TEXT NOT NULL DEFAULT '',
                workspace   TEXT,
                started_at  TEXT NOT NULL,
                ended_at    TEXT,
                duration_ms INTEGER DEFAULT 0
            );
            
            CREATE TABLE IF NOT EXISTS daily_summary (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                date          TEXT NOT NULL,
                class         TEXT NOT NULL,
                total_ms      INTEGER NOT NULL DEFAULT 0,
                session_count INTEGER NOT NULL DEFAULT 0,
                UNIQUE(date, class)
            );
            
            CREATE TABLE IF NOT EXISTS ai_conversations (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                role       TEXT NOT NULL,
                content    TEXT NOT NULL,
                model      TEXT NOT NULL
            );
            
            CREATE INDEX IF NOT EXISTS idx_sessions_class ON sessions(class);
            CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);
            CREATE INDEX IF NOT EXISTS idx_daily_summary_date ON daily_summary(date);"
        )?;
        Ok(())
    }

    /// 开始新的窗口 session，返回 session id
    pub fn start_session(&self, class: &str, title: &str, workspace: &str) -> anyhow::Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sessions (class, title, workspace, started_at) VALUES (?1, ?2, ?3, ?4)",
            params![class, title, workspace, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 结束 session，更新 ended_at 和 duration_ms
    pub fn end_session(&self, session_id: i64) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();
        let date_str = now.format("%Y-%m-%d").to_string();
        
        // 查询 started_at
        let started_at: String = self.conn.query_row(
            "SELECT started_at FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        
        let started: chrono::DateTime<chrono::Utc> = 
            chrono::DateTime::parse_from_rfc3339(&started_at)?.with_timezone(&chrono::Utc);
        let duration_ms = (now - started).num_milliseconds();
        
        // 更新 session
        self.conn.execute(
            "UPDATE sessions SET ended_at = ?1, duration_ms = ?2 WHERE id = ?3",
            params![now_str, duration_ms, session_id],
        )?;
        
        // 获取 class 用于更新 daily_summary
        let class: String = self.conn.query_row(
            "SELECT class FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        
        // 更新 daily_summary (upsert)
        self.conn.execute(
            "INSERT INTO daily_summary (date, class, total_ms, session_count)
             VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(date, class) DO UPDATE SET
               total_ms = total_ms + ?3,
               session_count = session_count + 1",
            params![date_str, class, duration_ms],
        )?;
        
        Ok(())
    }

    /// 获取当前正在进行的 session（如果有）
    pub fn current_session_id(&self) -> anyhow::Result<Option<i64>> {
        let result = self.conn.query_row(
            "SELECT id FROM sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        );
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
```

验证: cargo check -p hyprtrace-daemon

### Task 1.4: 实现 Hyprland 事件监听模块

创建 crates/hyprtrace-daemon/src/listener.rs：

```rust
use crate::config::Config;
use crate::db::Database;
use anyhow::Context;
use hyprland::event_listener::EventListener;
use std::sync::{Arc, Mutex};

pub struct WindowTracker {
    db: Arc<Mutex<Database>>,
    current_session_id: Option<i64>,
    current_class: Option<String>,
    config: Config,
}

impl WindowTracker {
    pub fn new(db: Database, config: Config) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
            current_session_id: None,
            current_class: None,
            config,
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let db = self.db.clone();
        let mut listener = EventListener::new();

        listener.add_active_window_changed_handler(move |data: Option<hyprland::event_listener::WindowEventData>| {
            let db = db.lock().unwrap();
            let now = chrono::Utc::now().to_rfc3339();

            match data {
                Some(win_data) => {
                    let class = win_data.class.to_lowercase();
                    let title = &win_data.title;
                    
                    // 如果当前有活跃 session 且 class 相同（同一应用不同窗口），不切换
                    // 如果 class 不同，结束旧 session，开始新 session
                    // 简化处理：总是结束旧 session，开始新 session
                    
                    if let Some(prev_id) = _end_current_session_inner(&db) {
                        log::info!("结束 session {}", prev_id);
                    }
                    
                    match db.start_session(&class, title, "") {
                        Ok(new_id) => {
                            log::info!("开始 session {}: class={}, title={}", new_id, class, title);
                        }
                        Err(e) => log::error!("开始 session 失败: {}", e),
                    }
                }
                None => {
                    // 无活跃窗口 → 结束当前 session，标记为空闲
                    if let Some(prev_id) = _end_current_session_inner(&db) {
                        log::info!("切换到空闲，结束 session {}", prev_id);
                    }
                    log::debug!("进入空闲状态");
                }
            }
        });

        log::info!("HyprTrace 守护进程已启动，正在监听窗口切换事件...");
        listener.start_listener().context("无法启动 Hyprland 事件监听器！请确认 Hyprland 正在运行。")
    }
}

/// 结束数据库中最新的未结束 session
fn _end_current_session_inner(db: &Database) -> Option<i64> {
    db.current_session_id().ok().flatten().and_then(|id| {
        db.end_session(id).ok().map(|_| id)
    })
}
```

验证: cargo check -p hyprtrace-daemon

### Task 1.5: 守护进程 main.rs 整合

修改 crates/hyprtrace-daemon/src/main.rs：

```rust
mod config;
mod db;
mod listener;

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    log::info!("HyprTrace 守护进程启动中...");

    // 加载配置
    let config = config::Config::load().context("加载配置失败")?;
    config.ensure_db_dir().context("创建数据库目录失败")?;

    // 打开数据库
    let db_path = config.db_path_expanded();
    let db = db::Database::open(&db_path).context("打开数据库失败")?;
    db.migrate().context("数据库迁移失败")?;
    log::info!("数据库已就绪: {:?}", db_path);

    // 处理优雅关闭
    let (tx, rx) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || {
        log::info!("收到终止信号，正在优雅关闭...");
        tx.send(()).ok();
    })?;

    // 启动窗口追踪
    let mut tracker = listener::WindowTracker::new(db, config);
    
    // 在独立线程中运行监听器
    std::thread::spawn(move || {
        if let Err(e) = tracker.run() {
            log::error!("窗口追踪器异常退出: {}", e);
        }
    });

    // 等待终止信号
    rx.recv().ok();
    log::info!("HyprTrace 守护进程已退出");
    Ok(())
}
```

注意: 需要在 Cargo.toml 添加 `ctrlc = "3"` 依赖。

验证: cargo build -p hyprtrace-daemon
```

---

## Phase 2 提示词：API 服务器

```
【Phase 2: API 服务器实现】

继续实现 hyprtrace-server。所有代码放在 crates/hyprtrace-server/src/ 下。

### Task 2.1: Server 端配置和数据库模块

复用 daemon 的 config.rs 结构（可复制到 server crate 或提取为共享 crate）。
如选择复制，调整 Config::load() 使其支持读取配置文件（只读，不自动创建）。

创建 crates/hyprtrace-server/src/db.rs：

实现以下函数（只读数据库连接，用于查询）：

```rust
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> anyhow::Result<Self> { ... }
    
    /// 今日概览
    pub fn today_summary(&self, date: &str) -> anyhow::Result<TodaySummary> {
        // 查询 daily_summary WHERE date = ? 的 SUM(total_ms), COUNT(DISTINCT class), SUM(session_count)
        // 同时查询今日 session 中最大间隔超过 idle_timeout 的部分作为 idle 时间
    }
    
    /// 应用排行 (按天/周/月聚合)
    pub fn app_ranking(&self, from: &str, to: &str, limit: usize) -> anyhow::Result<Vec<AppRank>> {
        // SELECT class, SUM(total_ms) as total_ms, SUM(session_count) as sessions
        // FROM daily_summary WHERE date BETWEEN ? AND ? GROUP BY class ORDER BY total_ms DESC LIMIT ?
    }
    
    /// 每小时明细
    pub fn hourly_breakdown(&self, date: &str) -> anyhow::Result<Vec<HourlyBucket>> {
        // 从 sessions 表中查询当天数据，按时段分桶
        // SELECT CAST(strftime('%H', started_at) AS INTEGER) as hour,
        //        SUM(CASE WHEN ended_at IS NULL THEN ... ELSE duration_ms END) as total_ms
        // FROM sessions WHERE date(started_at) = ?
        // GROUP BY hour ORDER BY hour
    }
    
    /// Session 分页列表
    pub fn sessions_paginated(&self, from: &str, to: &str, page: u32, per_page: u32) -> anyhow::Result<(Vec<Session>, u32)>;
    
    /// 单个应用的每日趋势
    pub fn app_daily_trend(&self, class: &str, from: &str, to: &str) -> anyhow::Result<Vec<DailyTrend>>;
    
    /// AI 对话
    pub fn save_ai_message(&self, role: &str, content: &str, model: &str) -> anyhow::Result<()>;
    pub fn ai_conversations(&self, limit: usize) -> anyhow::Result<Vec<AiMessage>>;
}
```

返回类型定义在 models.rs 中（全部 derive Serialize）:

```rust
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TodaySummary {
    pub date: String,
    pub total_active_ms: i64,
    pub total_idle_ms: i64,
    pub app_count: usize,
    pub session_count: i64,
    pub top_apps: Vec<AppRank>,
}

#[derive(Debug, Serialize)]
pub struct AppRank {
    pub class: String,
    pub total_ms: i64,
    pub percentage: f64,
    pub session_count: i64,
}

#[derive(Debug, Serialize)]
pub struct HourlyBucket {
    pub hour: u8,
    pub total_ms: i64,
    pub session_count: i64,
}

#[derive(Debug, Serialize)]
pub struct Session {
    pub id: i64,
    pub class: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DailyTrend {
    pub date: String,
    pub total_ms: i64,
    pub session_count: i64,
}

#[derive(Debug, Serialize)]
pub struct AiMessage {
    pub id: i64,
    pub created_at: String,
    pub role: String,
    pub content: String,
    pub model: String,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: u32,
    pub page: u32,
    pub per_page: u32,
}
```

### Task 2.2: 实现数据 API 路由

创建 crates/hyprtrace-server/src/routes/data.rs：

使用 Axum，实现以下 handlers：

```rust
use axum::{extract::{Query, State, Path}, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SummaryQuery {
    pub date: Option<String>,  // 默认今天
}

#[derive(Deserialize)]
pub struct AppRankingQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize { 10 }

#[derive(Deserialize)]
pub struct DateQuery {
    pub date: Option<String>,
}

#[derive(Deserialize)]
pub struct SessionQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 50 }

#[derive(Deserialize)]
pub struct AppTrendQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}
```

Handler 函数示例：
```rust
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "version": "0.1.0"}))
}

pub async fn summary(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SummaryQuery>,
) -> Result<Json<TodaySummary>, AppError> {
    let date = query.date.unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    let result = state.db.today_summary(&date)?;
    Ok(Json(result))
}
```

创建 routes/mod.rs 注册路由：
```rust
use axum::{Router, routing::get};
use std::sync::Arc;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(data::health))
        .route("/api/summary", get(data::summary))
        .route("/api/apps", get(data::app_ranking))
        .route("/api/timeline", get(data::timeline))
        .route("/api/sessions", get(data::sessions))
        .route("/api/app/{class}/trend", get(data::app_trend))
        .with_state(state)
}
```

### Task 2.3: 实现 AI 集成模块

创建 crates/hyprtrace-server/src/ai/mod.rs, ollama.rs, openai.rs：

```rust
// ai/mod.rs
use async_trait::async_trait;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,     // "system" | "user" | "assistant"
    pub content: String,
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn chat(&self, messages: &[ChatMessage]) -> anyhow::Result<String>;
    async fn list_models(&self) -> anyhow::Result<Vec<String>>;
    fn name(&self) -> &str;
}

pub struct AiManager {
    providers: std::collections::HashMap<String, Box<dyn AiProvider>>,
    pub default_provider: String,
    pub system_prompt: String,
}
```

Ollama 实现 (ai/ollama.rs):
```rust
use super::*;

pub struct OllamaProvider {
    pub base_url: String,
    pub default_model: String,
    client: reqwest::Client,
}

#[async_trait]
impl AiProvider for OllamaProvider {
    fn name(&self) -> &str { "ollama" }
    
    async fn chat(&self, messages: &[ChatMessage]) -> anyhow::Result<String> {
        let url = format!("{}/api/chat", self.base_url);
        let body = serde_json::json!({
            "model": self.default_model,
            "messages": messages,
            "stream": false
        });
        
        let resp = self.client.post(&url).json(&body).send().await?;
        let json: serde_json::Value = resp.json().await?;
        
        Ok(json["message"]["content"].as_str()
            .unwrap_or("(empty response)")
            .to_string())
    }
    
    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let json: serde_json::Value = resp.json().await?;
        let models = json["models"].as_array()
            .map(|arr| arr.iter()
                .filter_map(|m| m["name"].as_str().map(String::from))
                .collect())
            .unwrap_or_default();
        Ok(models)
    }
}
```

OpenAI 实现 (ai/openai.rs):
```rust
use super::*;

pub struct OpenAiProvider {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
    client: reqwest::Client,
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn name(&self) -> &str { "openai" }
    
    async fn chat(&self, messages: &[ChatMessage]) -> anyhow::Result<String> {
        let url = format!("{}/chat/completions", self.base_url);
        let api_key = if self.api_key.is_empty() {
            std::env::var("OPENAI_API_KEY").unwrap_or_default()
        } else {
            self.api_key.clone()
        };
        
        let body = serde_json::json!({
            "model": self.default_model,
            "messages": messages,
        });
        
        let resp = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await?;
        
        let json: serde_json::Value = resp.json().await?;
        Ok(json["choices"][0]["message"]["content"].as_str()
            .unwrap_or("(empty response)")
            .to_string())
    }
    
    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        // OpenAI API 的 /models 端点列出所有模型
        let url = format!("{}/models", self.base_url);
        let api_key = if self.api_key.is_empty() {
            std::env::var("OPENAI_API_KEY").unwrap_or_default()
        } else {
            self.api_key.clone()
        };
        
        let resp = self.client.get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await?;
        
        let json: serde_json::Value = resp.json().await?;
        let models = json["data"].as_array()
            .map(|arr| arr.iter()
                .filter_map(|m| m["id"].as_str().map(String::from))
                .collect())
            .unwrap_or_default();
        Ok(models)
    }
}
```

### Task 2.4: 实现 AI 路由

创建 crates/hyprtrace-server/src/routes/ai.rs：

```rust
POST /api/ai/chat
  Body: { "provider"?: string, "message": string, "include_data"?: bool, "date_range"?: string }
  
GET /api/ai/models
  Response: { "providers": { "ollama": [...], "openai": [...] }, "default": "ollama" }
```

聊天 handler 的核心逻辑：
1. 解析请求参数
2. 如果 include_data=true，查询对应时间范围的数据，构造 context string
3. 构造 messages: [system_prompt + data_context, ...历史对话, user_message]
4. 调用 AiManager 的对应 provider.chat()
5. 保存对话到数据库
6. 返回 { reply, model }

### Task 2.5: Server main.rs 整合

```rust
mod config;
mod db;
mod models;
mod routes;
mod ai;

use std::sync::Arc;
use axum::Router;
use tower_http::cors::CorsLayer;

pub struct AppState {
    pub db: db::Database,
    pub ai: ai::AiManager,
    pub config: config::Config,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    
    let config = config::Config::load()?;
    let db = db::Database::open(&config.db_path_expanded())?;
    let ai = ai::AiManager::from_config(&config.ai, Arc::new(/* db clone? */))?;
    
    let state = Arc::new(AppState { db, ai, config });
    let router = routes::create_router(state)
        .layer(CorsLayer::permissive());
    
    let addr = format!("{}:{}", config.server.host, config.server.port);
    log::info!("HyprTrace API 服务器启动于 http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}
```

验证: cargo build -p hyprtrace-server
```

---

## Phase 3 提示词：Web 前端

```
【Phase 3: Web 前端实现】

在 /root/hyprtrace/web/ 下创建 React + TypeScript + Vite 项目。

### Task 3.1: 项目初始化

手动创建项目结构（无需 vite create，直接编写文件）:

web/package.json:
```json
{
  "name": "hyprtrace-web",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-router-dom": "^6.26.0",
    "recharts": "^2.12.0",
    "lucide-react": "^0.400.0",
    "date-fns": "^3.6.0",
    "react-markdown": "^9.0.0"
  },
  "devDependencies": {
    "@types/react": "^18.3.3",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.1",
    "autoprefixer": "^10.4.19",
    "postcss": "^8.4.38",
    "tailwindcss": "^3.4.4",
    "typescript": "^5.5.3",
    "vite": "^5.3.4"
  }
}
```

web/vite.config.ts:
```typescript
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:9420'
    }
  }
})
```

web/tsconfig.json: 标准 React TS 配置，strict: true
web/tailwind.config.js: 暗色主题，自定义配色
web/postcss.config.js: tailwind + autoprefixer
web/index.html: 标准 HTML5 入口

验证: cd web && npm install && npm run dev

### Task 3.2: 全局样式和布局

使用 Tailwind CSS 暗色主题（默认暗色）:
- 背景色: bg-gray-950
- 卡片: bg-gray-900 border-gray-800
- 侧边栏: 左侧固定 240px，深色背景
- 主内容区: 剩余空间

配色方案（Hyprland 风格）:
- 主色: cyan-400 (#22d3ee)  
- 强调: blue-500, purple-500, emerald-400
- 文字: gray-100, gray-400

创建 Layout 组件（侧边栏 + 主内容区 + Outlet）：
- 侧边栏导航: 仪表盘 / 应用排行 / 时间线 / Sessions / AI 分析 / 设置
- 使用 Lucide Icons: LayoutDashboard, BarChart3, Clock, List, Bot, Settings
- 底部显示版本号和守护进程状态

### Task 3.3: API 客户端和类型

创建 web/src/lib/types.ts (所有 API 返回类型):
- TodaySummary, AppRank, HourlyBucket, Session, DailyTrend, AiMessage, AiResponse, AiModelsResponse, PaginatedResponse

创建 web/src/lib/api.ts:
```typescript
const BASE = '';  // 使用 Vite proxy，不需要完整 URL

async function fetchJSON<T>(url: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${url}`, options);
  if (!res.ok) throw new Error(`API Error: ${res.status} ${await res.text()}`);
  return res.json();
}

export const api = {
  health: () => fetchJSON<{ status: string }>('/api/health'),
  summary: (date: string) => fetchJSON<TodaySummary>(`/api/summary?date=${date}`),
  appRanking: (from: string, to: string, limit = 10) =>
    fetchJSON<AppRank[]>(`/api/apps?from=${from}&to=${to}&limit=${limit}`),
  timeline: (date: string) => fetchJSON<HourlyBucket[]>(`/api/timeline?date=${date}`),
  sessions: (from: string, to: string, page = 1, perPage = 50) =>
    fetchJSON<PaginatedResponse<Session>>(`/api/sessions?from=${from}&to=${to}&page=${page}&per_page=${perPage}`),
  appTrend: (cls: string, from: string, to: string) =>
    fetchJSON<DailyTrend[]>(`/api/app/${encodeURIComponent(cls)}/trend?from=${from}&to=${to}`),
  aiModels: () => fetchJSON<AiModelsResponse>('/api/ai/models'),
  aiChat: (provider: string, message: string, includeData: boolean, dateRange: string) =>
    fetchJSON<AiResponse>('/api/ai/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ provider, message, include_data: includeData, date_range: dateRange }),
    }),
};
```

### Task 3.4: Dashboard 页面

实现 web/src/pages/Dashboard.tsx:

布局:
```
┌────────────────────────────────────────────────────┐
│  📊 仪表盘                          2026-07-12     │
├──────────┬──────────┬──────────┬──────────────────┤
│ 活跃时间  │  应用数   │ 会话数    │  空闲时间        │
│ 5h 23m   │    12    │   47     │  2h 15m          │
│ StatCard │ StatCard │ StatCard │  StatCard        │
├──────────┴──────────┴──────────┴──────────────────┤
│  应用使用分布                    │  应用排行        │
│  ┌──────────────────┐          │  Firefox  3h12m  │
│  │   PieChart       │          │  Code     1h05m  │
│  │   (Recharts)     │          │  Terminal   45m  │
│  └──────────────────┘          │  Discord    32m  │
├───────────────────────────────────────────────────┤
│  24小时活跃度                                    │
│  0  2  4  6  8  10 12 14 16 18 20 22            │
│  ░░ ░░ ▒▒ ▒▒ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▒▒ ▒▒ ░░           │
│  (Recharts BarChart)                             │
└───────────────────────────────────────────────────┘
```

关键实现:
- 使用 `date-fns/format` 格式化日期
- 使用 `useEffect` + `useState` 加载数据
- StatCard: 传 icon, label, value, subtext
- 饼图颜色映射: 每个 class 一个固定颜色 (从预定义色板中取)
- 柱状图: 24 个柱子，按小时分布

### Task 3.5: 应用排行页面

实现 web/src/pages/Apps.tsx:

- 顶部: 时间范围选择器 (今天 | 本周 | 本月 | 自定义日期范围)
- 主要内容: 水平条形图（Recharts BarChart horizontal）
- 每条: 应用名 | 时长 | 占比% | 条形
- 点击应用行 → 展开下方每日趋势折线图（该应用最近 7/30 天）

### Task 3.6: 时间线页面

实现 web/src/pages/Timeline.tsx:

- 24 小时堆叠柱状图，按应用着色
- X 轴: 0-23 小时
- Y 轴: 活跃时长（分钟）
- 每个柱子的各段代表不同应用
- 日期选择器切换不同天
- Hover 显示该小时各应用详细时长

### Task 3.7: Sessions 页面

实现 web/src/pages/Sessions.tsx:

- 表格显示 session 列表（时间、应用、标题、时长）
- 按应用 class 筛选
- 分页（前端分页或后端分页）
- 应用图标/颜色标签

### Task 3.8: AI 对话面板

实现 web/src/pages/AIChat.tsx:

聊天界面:
```
┌──────────────────────────────────────────────────┐
│  🤖 AI 分析助手                    [模型选择 ▼]   │
├──────────────────────────────────────────────────┤
│                                                  │
│  ┌────────────────────────────────────────────┐  │
│  │ (AI) 你好！我是 HyprTrace 分析助手。        │  │
│  │ 我可以帮你分析窗口使用数据。                  │  │
│  └────────────────────────────────────────────┘  │
│                                                  │
│              ┌──────────────────────────────┐    │
│              │ 我今天哪些应用用了最长时间？    │    │
│              └──────────────────────────────┘    │
│                                                  │
│  ┌────────────────────────────────────────────┐  │
│  │ (AI) 根据今天数据，你使用最多的是...         │  │
│  └────────────────────────────────────────────┘  │
│                                                  │
├──────────────────────────────────────────────────┤
│ [✅ 附带今日数据]  快捷: 效率分析 | 浪费时间 | ... │
│ [输入框..................................]  [发送] │
└──────────────────────────────────────────────────┘
```

关键实现:
- 消息列表使用 react-markdown 渲染
- "附带数据" toggle: 开启时请求自动附上数据库查询结果作为 context
- 模型选择器: GET /api/ai/models 获取可用模型列表
- 快捷问题按钮: 预设中文问题
- auto-scroll 到最底部
- Loading 状态: 发送时显示 typing indicator

### Task 3.9: 设置页面

实现 web/src/pages/Settings.tsx:

- 服务器状态指示（调用 /api/health）
- 数据库路径和大小
- AI 提供者状态（Ollama 连接测试、OpenAI API key 状态）
- 主题切换（暗色/亮色）
- 数据导出按钮

### Task 3.10: App.tsx 和路由

```tsx
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import Dashboard from './pages/Dashboard';
import Apps from './pages/Apps';
import Timeline from './pages/Timeline';
import Sessions from './pages/Sessions';
import AIChat from './pages/AIChat';
import Settings from './pages/Settings';

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<Dashboard />} />
          <Route path="/apps" element={<Apps />} />
          <Route path="/timeline" element={<Timeline />} />
          <Route path="/sessions" element={<Sessions />} />
          <Route path="/ai" element={<AIChat />} />
          <Route path="/settings" element={<Settings />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
```

验证: npm run build (确保 TypeScript 编译通过)
```

---

## Phase 4 提示词：安装脚本和最终打磨

```
【Phase 4: 集成脚本和部署】

### Task 4.1: 创建 systemd user service 文件

创建 /root/hyprtrace/scripts/hyprtrace-daemon.service:
```ini
[Unit]
Description=HyprTrace Window Time Tracker Daemon
After=graphical-session.target
PartOf=graphical-session.target
Requires=graphical-session.target

[Service]
Type=simple
ExecStart=%h/.local/bin/hyprtrace-daemon
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=graphical-session.target
```

创建 /root/hyprtrace/scripts/hyprtrace-server.service:
```ini
[Unit]
Description=HyprTrace API Server
After=network.target

[Service]
Type=simple
ExecStart=%h/.local/bin/hyprtrace-server
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
```

### Task 4.2: 创建 install.sh

创建 /root/hyprtrace/scripts/install.sh:
```bash
#!/bin/bash
set -e

echo "=== HyprTrace 安装脚本 ==="

# 检查依赖
command -v cargo >/dev/null 2>&1 || { echo "❌ 需要 Rust toolchain"; exit 1; }
command -v node >/dev/null 2>&1 || { echo "❌ 需要 Node.js"; exit 1; }
command -v npm >/dev/null 2>&1 || { echo "❌ 需要 npm"; exit 1; }

# 编译 Rust
echo "📦 编译 Rust 组件..."
cargo build --release
cp target/release/hyprtrace-daemon ~/.local/bin/
cp target/release/hyprtrace-server ~/.local/bin/

# 构建前端
echo "📦 构建前端..."
cd web
npm install
npm run build
mkdir -p ~/.local/share/hyprtrace/web
cp -r dist/* ~/.local/share/hyprtrace/web/
cd ..

# 安装 systemd services
echo "📦 安装 systemd 服务..."
mkdir -p ~/.config/systemd/user/
cp scripts/hyprtrace-daemon.service ~/.config/systemd/user/
cp scripts/hyprtrace-server.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now hyprtrace-daemon.service
systemctl --user enable --now hyprtrace-server.service

echo "✅ HyprTrace 安装完成!"
echo "   前端: http://localhost:9420 (由 server 托管静态文件或独立 dev server)"
echo "   数据库: ~/.local/share/hyprtrace/hyprtrace.db"
echo "   配置: ~/.config/hyprtrace/config.toml"
```

### Task 4.3: 编写 README.md

创建 /root/hyprtrace/README.md，包含:
- 项目介绍和架构图（ASCII art）
- 依赖要求（Hyprland, Rust, Node.js, 可选 Ollama）
- 安装步骤
- 配置说明
- 截图占位
- 开发指南

### Task 4.4: Server 端静态文件托管（可选增强）

修改 server main.rs，增加静态文件服务:
```rust
use tower_http::services::ServeDir;

// 在 router 中添加 fallback
router.fallback_service(ServeDir::new(web_dir))
```

这样访问 http://localhost:9420 就直接打开前端。

### Task 4.5: 最终验证

完整流程:
1. cargo build --release (确保所有 Rust 代码编译通过)
2. cd web && npm run build (确保前端编译通过)
3. 手动启动 daemon: ./target/release/hyprtrace-daemon (需要 Hyprland 环境)
4. 手动启动 server: ./target/release/hyprtrace-server
5. 打开浏览器访问 http://localhost:9420
6. 切换几个窗口，等待几秒，刷新仪表盘看数据是否出现
7. 进入 AI 面板，测试 Ollama 连接
```

---

## 常见问题处理（Agent 自行排查）

1. **cargo build 失败: hyprland-rs 编译错误**
   - 检查 Rust 版本 ≥ 1.85.0 (hyprland-rs 使用 edition 2024)
   - 运行 `rustup update`

2. **daemon 启动报 "无法连接 Hyprland socket"**
   - 确认 `$HYPRLAND_INSTANCE_SIGNATURE` 环境变量已设置
   - 确认 Hyprland 正在运行
   - systemd service 需在 graphical-session.target 之后启动

3. **前端 API 请求 404**
   - 确认 server 在运行: `curl http://127.0.0.1:9420/api/health`
   - 检查 CORS 配置

4. **AI 对话无响应 (Ollama)**
   - 确认 Ollama 运行中: `ollama serve` 或 `systemctl status ollama`
   - 确认模型已下载: `ollama pull qwen2.5:7b`

5. **数据库中无数据**
   - 确认 daemon 在运行且无错误日志
   - 检查 `~/.local/share/hyprtrace/hyprtrace.db` 文件大小
   - 尝试切换几个窗口，等待 2-3 秒后检查

6. **空闲时间不准确**
   - 调整 config 中的 idle_timeout_seconds
   - 当前实现在 ActiveWindowChanged 返回 None 时标记空闲
