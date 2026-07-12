# HyprTrace — Hyprland Window Time Tracker 实现计划

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**目标:** 构建一个完整的 Hyprland 窗口时间追踪系统，包含后端守护进程（持续记录用户窗口使用时间）、美观的 Web 前端仪表盘（浏览和分析数据）、以及 AI 分析集成（本地 Ollama + 云端 API 双模式）。

**架构:** Rust workspace 包含两个 crate：`hyprtrace-daemon`（基于 hyprland-rs IPC 事件监听的后台守护进程，持续写入 SQLite）和 `hyprtrace-server`（Axum REST API 服务，暴露数据查询接口和 AI 分析接口）。前端为独立的 React + TypeScript + Tailwind CSS + Recharts Web 应用，通过 API 与后端通信。AI 模块抽象为 trait，支持 Ollama 和 OpenAI 双实现。

**技术栈:**
- 守护进程: Rust, hyprland-rs 0.4.0-beta.3, rusqlite, tokio
- API 服务器: Rust, Axum, rusqlite, reqwest (AI 调用)
- 前端: React 18 + TypeScript, Vite, Tailwind CSS 3, Recharts, Lucide Icons
- 数据库: SQLite (单文件，存储在 ~/.local/share/hyprtrace/)
- AI: reqwest → Ollama (本地) / OpenAI API (云端)，通过配置文件切换

---

## 架构详解

### 数据流

```
Hyprland IPC Socket
       │
       ▼
┌──────────────────┐
│ hyprtrace-daemon │  ← 监听 ActiveWindowChanged / WindowClosed 等事件
│  (Rust binary)   │  ← 使用 hyprland-rs EventListener
│                  │  ← 计算窗口活跃时长 Δt
│                  │  ← 写入 SQLite
└──────┬───────────┘
       │ SQLite WAL
       ▼
┌──────────────────┐
│    SQLite DB     │  ← ~/.local/share/hyprtrace/hyprtrace.db
│                  │  ← 表: sessions, events, daily_summary
└──────┬───────────┘
       │ 读取
       ▼
┌──────────────────┐
│ hyprtrace-server │  ← Axum HTTP API (默认 127.0.0.1:9420)
│  (Rust binary)   │  ← GET  /api/summary?date=&range=
│                  │  ← GET  /api/sessions?from=&to=
│                  │  ← GET  /api/apps?period=daily|weekly|monthly
│                  │  ← POST /api/ai/chat  (发送分析 prompt)
│                  │  ← GET  /api/ai/models (列出可用模型)
└──────┬───────────┘
       │ HTTP/JSON
       ▼
┌──────────────────┐
│  hyprtrace-web   │  ← React SPA (Vite dev server 或 静态文件)
│  (React + TS)    │  ← 仪表盘、时间线、应用排行、AI 对话面板
└──────────────────┘
```

### 数据库 Schema

```sql
-- 窗口会话：记录每次窗口聚焦的时间段
CREATE TABLE sessions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    class       TEXT NOT NULL,           -- 窗口类名，如 "firefox"
    title       TEXT NOT NULL DEFAULT '',-- 窗口标题
    workspace   TEXT,                    -- 工作区名称
    started_at  TEXT NOT NULL,           -- ISO 8601 时间戳
    ended_at    TEXT,                    -- NULL 表示正在进行中
    duration_ms INTEGER DEFAULT 0       -- 时长（毫秒），结束时更新
);

-- 聚合统计：按天+应用汇总（加速查询）
CREATE TABLE daily_summary (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    date        TEXT NOT NULL,           -- YYYY-MM-DD
    class       TEXT NOT NULL,
    total_ms    INTEGER NOT NULL DEFAULT 0,
    session_count INTEGER NOT NULL DEFAULT 0,
    UNIQUE(date, class)
);

-- AI 对话历史（可选）
CREATE TABLE ai_conversations (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at  TEXT NOT NULL,
    role        TEXT NOT NULL,           -- user / assistant
    content     TEXT NOT NULL,
    model       TEXT NOT NULL
);

CREATE INDEX idx_sessions_class ON sessions(class);
CREATE INDEX idx_sessions_started ON sessions(started_at);
CREATE INDEX idx_daily_summary_date ON daily_summary(date);
```

### 配置文件

`~/.config/hyprtrace/config.toml`:

```toml
[daemon]
db_path = "~/.local/share/hyprtrace/hyprtrace.db"
idle_timeout_seconds = 300          # 超过5分钟无切换视为空闲
poll_interval_ms = 0                # 0 = 事件驱动（推荐）

[server]
host = "127.0.0.1"
port = 9420

[ai]
default_provider = "ollama"         # ollama | openai

[ai.ollama]
base_url = "http://localhost:11434"
default_model = "qwen2.5:7b"

[ai.openai]
api_key = ""                        # 留空则从 OPENAI_API_KEY 环境变量读取
base_url = "https://api.openai.com/v1"
default_model = "gpt-4o-mini"
system_prompt = "你是一个窗口使用时间分析助手。根据用户提供的应用使用数据，分析使用习惯、给出效率建议、识别潜在的时间浪费。用中文回答。"
```

---

## 项目结构

```
hyprtrace/
├── Cargo.toml                     (workspace)
├── README.md
├── crates/
│   ├── hyprtrace-daemon/          (Rust: 后台守护进程)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── db.rs
│   │       ├── listener.rs
│   │       └── models.rs
│   └── hyprtrace-server/          (Rust: API 服务器)
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── config.rs
│           ├── db.rs
│           ├── models.rs
│           ├── ai/
│           │   ├── mod.rs
│           │   ├── ollama.rs
│           │   └── openai.rs
│           └── routes/
│               ├── mod.rs
│               ├── data.rs
│               └── ai.rs
├── web/                           (React + TypeScript 前端)
│   ├── package.json
│   ├── vite.config.ts
│   ├── index.html
│   ├── tsconfig.json
│   ├── tailwind.config.ts
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── index.css
│       ├── lib/
│       │   ├── api.ts
│       │   └── types.ts
│       ├── components/
│       │   ├── Layout.tsx
│       │   ├── Sidebar.tsx
│       │   ├── StatCard.tsx
│       │   ├── AppUsagePie.tsx
│       │   ├── HourlyHeatmap.tsx
│       │   ├── AppRankingBar.tsx
│       │   ├── TimelineChart.tsx
│       │   ├── ChatMessage.tsx
│       │   └── ChatInput.tsx
│       └── pages/
│           ├── Dashboard.tsx
│           ├── Apps.tsx
│           ├── Timeline.tsx
│           ├── Sessions.tsx
│           ├── AIChat.tsx
│           └── Settings.tsx
└── scripts/
    ├── install.sh
    ├── hyprtrace-daemon.service
    └── hyprtrace-server.service
```

---

## 分阶段实现

### Phase 1: 项目初始化和基础设施

#### Task 1: 创建 Rust workspace 和项目骨架

初始化 Cargo workspace，创建 daemon 和 server 两个 crate。

**crates/hyprtrace-daemon/Cargo.toml:**
```toml
[package]
name = "hyprtrace-daemon"
version.workspace = true
edition.workspace = true

[dependencies]
hyprland = "0.4.0-beta.3"
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
directories = "5"
toml = "0.8"
log = "0.4"
env_logger = "0.11"
anyhow = "1"
```

**crates/hyprtrace-server/Cargo.toml:**
```toml
[package]
name = "hyprtrace-server"
version.workspace = true
edition.workspace = true

[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.31", features = ["bundled"] }
chrono = { version = "0.4", features = ["serde"] }
directories = "5"
toml = "0.8"
tower-http = { version = "0.5", features = ["cors"] }
reqwest = { version = "0.12", features = ["json"] }
anyhow = "1"
log = "0.4"
env_logger = "0.11"
uuid = { version = "1", features = ["v4"] }
async-trait = "0.1"
```

---

#### Task 2: 实现配置模块

实现 TOML 配置文件加载，daemon 和 server 共用相同结构。

```rust
// crates/hyprtrace-daemon/src/config.rs
use serde::Deserialize;
use directories::ProjectDirs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub daemon: DaemonConfig,
    pub server: ServerConfig,
    pub ai: AiConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DaemonConfig {
    pub db_path: String,
    pub idle_timeout_seconds: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AiConfig {
    pub default_provider: String,
    #[serde(default)]
    pub ollama: Option<OllamaConfig>,
    #[serde(default)]
    pub openai: Option<OpenAiConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OllamaConfig {
    pub base_url: String,
    pub default_model: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenAiConfig {
    #[serde(default)]
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> { ... }
    pub fn db_path_expanded(&self) -> std::path::PathBuf { ... }
    pub fn ensure_dirs(&self) -> anyhow::Result<()> { ... }
}
```

---

#### Task 3: 实现数据库模块 (daemon 端)

实现 SQLite 数据库初始化、migration、session CRUD。

核心函数签名:
```rust
pub struct Database { conn: rusqlite::Connection }

impl Database {
    pub fn open(path: &Path) -> anyhow::Result<Self>;
    pub fn migrate(&self) -> anyhow::Result<()>;
    pub fn start_session(&self, class: &str, title: &str, workspace: &str) -> anyhow::Result<i64>;
    pub fn end_session(&self, session_id: i64, ended_at: &str) -> anyhow::Result<()>;
    pub fn upsert_daily_summary(&self, date: &str, class: &str, ms: i64) -> anyhow::Result<()>;
}
```

关键实现细节:
- 使用 WAL 模式: `PRAGMA journal_mode=WAL;`
- busy_timeout: `PRAGMA busy_timeout=5000;`
- end_session: 计算 started_at 到 ended_at 的差值，更新 duration_ms 和 daily_summary

---

#### Task 4: 实现 Hyprland 事件监听模块

使用 hyprland-rs EventListener 监听窗口切换事件，核心逻辑：

```rust
use hyprland::event_listener::EventListener;
use hyprland::shared::HyprDataActive;
use hyprland::data::Client;
use hyprland::event_listener::WindowEventData;

pub struct WindowTracker {
    db: Arc<Mutex<Database>>,
    current_session_id: Option<i64>,
    current_class: Option<String>,
    idle: bool,
    config: Config,
}

impl WindowTracker {
    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut listener = EventListener::new();
        let db = self.db.clone();
        
        listener.add_active_window_changed_handler(move |data: Option<WindowEventData>| {
            let now = chrono::Utc::now();
            match data {
                Some(win_data) => {
                    // 结束上一个 session
                    // 开始新 session
                    // 更新 daily_summary
                }
                None => {
                    // 切换到空桌面 → 标记空闲
                }
            }
        });
        
        listener.start_listener()?;
        Ok(())
    }
}
```

关键设计:
- `ActiveWindowChanged` 提供 `Option<WindowEventData>`，None 表示无活跃窗口
- `WindowEventData` 包含 `class`, `title`, `address`
- 用 `class` 作为应用标识（规范化：全小写）
- 同一个 class 的不同 address 视为独立 session
- 空闲检测：超过 `idle_timeout_seconds` 无事件 = 空闲中

---

#### Task 5: 守护进程 main.rs 整合

将 config、db、listener 串联：
- 加载配置 → 打开/迁移数据库 → 启动 WindowTracker
- SIGTERM/SIGINT 处理：优雅关闭，结束当前 session
- 日志记录到文件

---

### Phase 2: API 服务器

#### Task 6: Server 端数据库查询模块

```rust
// crates/hyprtrace-server/src/db.rs
pub struct Database { conn: rusqlite::Connection }

impl Database {
    pub fn open_readonly(path: &Path) -> anyhow::Result<Self>;
    
    pub fn today_summary(&self, date: &str) -> anyhow::Result<TodaySummary>;
    // 返回: { total_active_ms, app_count, session_count, top_apps }
    
    pub fn app_ranking(&self, from: &str, to: &str, limit: usize) -> anyhow::Result<Vec<AppRank>>;
    // SELECT class, SUM(total_ms), SUM(session_count) FROM daily_summary
    // WHERE date BETWEEN ? AND ? GROUP BY class ORDER BY total_ms DESC
    
    pub fn hourly_breakdown(&self, date: &str) -> anyhow::Result<Vec<HourlyBucket>>;
    // 将 sessions 按时段分桶 (0-23)，统计每小时活跃毫秒数
    
    pub fn sessions_paginated(&self, from: &str, to: &str, offset: u32, limit: u32) -> anyhow::Result<(Vec<Session>, u32)>;
    
    pub fn app_daily_trend(&self, class: &str, from: &str, to: &str) -> anyhow::Result<Vec<DailyTrend>>;
    
    pub fn save_ai_message(&self, role: &str, content: &str, model: &str) -> anyhow::Result<()>;
    pub fn ai_conversations(&self, limit: usize) -> anyhow::Result<Vec<AiMessage>>;
}
```

---

#### Task 7: 数据 API 路由

Axum handler 实现：

```
GET  /api/health                          → { "status": "ok" }
GET  /api/summary?date=2026-07-12         → TodaySummary
GET  /api/apps?from=...&to=...&limit=10   → Vec<AppRank>
GET  /api/timeline?date=2026-07-12        → Vec<HourlyBucket>
GET  /api/sessions?from=...&to=...&page=1&per_page=50 → { sessions, total, page }
GET  /api/app/{class}/trend?from=...&to=... → Vec<DailyTrend>
```

使用 Axum `State<Arc<AppState>>` 共享数据库连接。
使用 `tower-http::cors::CorsLayer::permissive()` 允许前端跨域。

---

#### Task 8: AI 集成模块

抽象 trait 实现双后端：

```rust
// crates/hyprtrace-server/src/ai/mod.rs
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn chat(&self, messages: &[ChatMessage]) -> anyhow::Result<String>;
    async fn list_models(&self) -> anyhow::Result<Vec<String>>;
    fn name(&self) -> &str;
}

pub struct AiManager {
    providers: HashMap<String, Box<dyn AiProvider>>,
    default_provider: String,
    system_prompt: String,
}
```

**Ollama 实现:**
- POST `{base_url}/api/chat` → `{ "model": "...", "messages": [...], "stream": false }`
- 解析响应 → message.content

**OpenAI 实现:**
- POST `{base_url}/chat/completions` with `Authorization: Bearer {api_key}`
- 解析响应 → choices[0].message.content

---

#### Task 9: AI 路由

```
GET  /api/ai/models  → { "providers": {"ollama": ["qwen2.5:7b", ...], "openai": [...]}, "default": "ollama" }

POST /api/ai/chat
  Body: { "provider": "ollama", "message": "...", "include_data": true, "date_range": "today" }
  Response: { "reply": "...", "model": "..." }
```

实现细节:
- `include_data=true` 时，自动从 DB 查询对应的使用数据，构造 context 注入 system message
- context 格式: 结构化 JSON 数据摘要，让 AI 容易理解
- 保存对话历史到 `ai_conversations` 表

---

#### Task 10: Server main.rs 整合 + systemd service

- 整合路由、启动 Axum server
- 创建 systemd user service 文件
- 创建 install.sh 脚本

```ini
# hyprtrace-daemon.service
[Unit]
Description=HyprTrace Window Time Tracker Daemon
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=%h/.local/bin/hyprtrace-daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=graphical-session.target
```

---

### Phase 3: Web 前端 (React + TypeScript + Tailwind)

#### Task 11: 初始化 Vite + React + TypeScript 项目

```bash
npm create vite@latest web -- --template react-ts
cd web
npm install react-router-dom recharts lucide-react date-fns
npm install -D tailwindcss @tailwindcss/vite
```

配置 Tailwind CSS 暗色主题（Hyprland 风格）。

---

#### Task 12: 布局 + 路由

侧边栏导航 + 6 个页面路由:
- `/` → Dashboard
- `/apps` → Apps
- `/timeline` → Timeline
- `/sessions` → Sessions
- `/ai` → AIChat
- `/settings` → Settings

---

#### Task 13: API 客户端层

```typescript
// web/src/lib/api.ts
const BASE = 'http://127.0.0.1:9420';

export async function fetchSummary(date: string): Promise<TodaySummary> { ... }
export async function fetchAppRanking(from: string, to: string, limit: number): Promise<AppRank[]> { ... }
export async function fetchTimeline(date: string): Promise<HourlyBucket[]> { ... }
export async function fetchSessions(from: string, to: string, page: number): Promise<PaginatedSessions> { ... }
export async function sendAiMessage(provider: string, message: string, includeData: boolean, dateRange: string): Promise<AiResponse> { ... }
export async function fetchAiModels(): Promise<AiModelsResponse> { ... }
```

---

#### Task 14: Dashboard 页面

4 个统计卡片 + 饼图 + 每小时间热力图。
使用 Recharts 的 PieChart, BarChart 组件。

---

#### Task 15: 应用排行页面

时间段选择器（今天/本周/本月/自定义） + 水平条形图。

---

#### Task 16: 时间线页面

24 小时柱状图，按应用分类堆叠颜色。

---

#### Task 17: AI 对话面板

聊天 UI:
- 消息列表（支持 Markdown 渲染）
- 输入框 + 发送按钮
- "附带数据" toggle
- 模型选择器
- 预设快捷问题
- 自动滚动到最新消息

---

#### Task 18: 设置页面 + 最终打磨

- 守护进程/服务器状态指示
- 数据库信息
- 主题切换
- 数据导出
- Loading skeleton / Error boundary / 空状态
- favicon 和中文本地化

---

## 风险和注意事项

1. **hyprland-rs 版本:** 0.4.0-beta.3 是 beta 版，建议锁定版本，关注 Hyprland 更新
2. **IPC socket:** daemon 需要 `$HYPRLAND_INSTANCE_SIGNATURE` 环境变量，systemd user service 需在 graphical-session.target 之后启动
3. **SQLite 并发:** WAL 模式 + busy_timeout 处理读写并发
4. **空闲检测:** ActiveWindowChanged 在锁屏时可能不触发，需额外处理
5. **XWayland:** 窗口 class/title 可能为乱码或不完整，需做容错
6. **隐私:** 窗口标题可能含敏感信息，建议加开关控制是否记录 title
