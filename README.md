# HyprTrace

> Hyprland Window Time Tracker — 追踪你的窗口使用时间，用数据提升效率

HyprTrace 是一个 Hyprland 窗口时间追踪系统，持续记录你在每个应用上花费的时间，提供美观的 Web 仪表盘进行数据分析，并集成本地 Ollama 和云端 OpenAI 兼容 API 的 AI 分析功能。

## 预览

| Dashboard | Apps |
|---|---|
| ![Dashboard](imgs/Dashiboard.png) | ![Apps](imgs/apps.png) |

| AI Chat | Sessions |
|---|---|
| ![AI Chat](imgs/AIChats.png) | ![Sessions](imgs/sessions.png) |

## 架构

```
Hyprland IPC Socket
       │
       ▼
┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│ hyprtrace-daemon │────▶│     SQLite       │◀────│ hyprtrace-server │
│  (Rust binary)   │     │  (WAL mode)      │     │  (Axum API)      │
│  事件监听 + 写入  │     │                  │     │  REST API + 静态文件 │
└──────────────────┘     └──────────────────┘     └────────┬─────────┘
                                                           │
                                                    ┌──────▼──────┐
                                                    │  Web 前端    │
                                                    │ React + TS  │
                                                    │ Tailwind +  │
                                                    │ Recharts    │
                                                    └─────────────┘
```

**三个组件：**

- **`hyprtrace-daemon`** — Rust 后台守护进程，监听 Hyprland `ActiveWindowChanged` 事件，记录窗口焦点会话到 SQLite 数据库
- **`hyprtrace-server`** — Rust Axum HTTP API 服务器，读取 SQLite 数据，暴露 REST 接口（数据查询 + AI 对话），同时提供前端静态文件服务
- **`hyprtrace-web`** — React + TypeScript + Tailwind CSS + Recharts 前端 SPA，包含仪表盘、应用排行、时间线、会话浏览和 AI 分析面板

## 功能

- **自动追踪** — 守护进程自动记录每个窗口的使用时长，无需手动操作
- **仪表盘** — 今日活跃时间、应用数量、会话数量、空闲时间一览
- **应用排行** — 按日/周/月查看各应用使用时长排名，支持点击查看趋势图
- **24h 时间线** — 以图表形式展示每天 24 小时的活动分布
- **会话记录** — 分页浏览所有窗口切换历史，支持按应用筛选
- **AI 分析** — 集成 Ollama 本地模型和 OpenAI 兼容 API（支持 DeepSeek、Groq 等），分析使用习惯和效率建议
- **数据导出** — 支持导出 CSV 格式的会话数据
- **Web 配置** — 在设置页面直接配置 AI API 地址、密钥和模型，无需编辑配置文件

## 安装

### 依赖

- [Rust 工具链](https://rustup.rs/) (cargo)
- [Node.js](https://nodejs.org/) + npm
- [Hyprland](https://hyprland.org/) Wayland 合成器

### 一键安装

```bash
git clone https://github.com/yourusername/hyprtrace.git
cd hyprtrace
bash scripts/install.sh
```

安装脚本会：
1. 编译 Rust 后端 (`hyprtrace-daemon` + `hyprtrace-server`)
2. 构建前端并复制到 `~/.local/share/hyprtrace/web/`
3. 安装并启动 systemd 用户服务

### 卸载

```bash
bash scripts/uninstall.sh           # 移除程序文件和服务
bash scripts/uninstall.sh --data    # 同时删除数据库
bash scripts/uninstall.sh --config  # 同时删除配置文件
bash scripts/uninstall.sh --all     # 删除所有文件（程序 + 数据 + 配置）
```

## 使用

安装完成后：

- **Web 仪表盘**: 打开浏览器访问 `http://localhost:9420`
- **数据库**: `~/.local/share/hyprtrace/hyprtrace.db`
- **配置文件**: `~/.config/hyprtrace/config.toml`

### 手动启动

```bash
# 启动守护进程（需要 Hyprland 运行中）
hyprtrace-daemon

# 启动 API 服务器
hyprtrace-server

# 开发模式前端（带热更新，代理 API 到 9420）
cd web && npm run dev
```

## 配置

配置文件位于 `~/.config/hyprtrace/config.toml`，首次运行自动生成：

```toml
[daemon]
db_path = "~/.local/share/hyprtrace/hyprtrace.db"
idle_timeout_seconds = 300

[server]
host = "127.0.0.1"
port = 9420

[ai]
default_provider = "ollama"

[ai.ollama]
base_url = "http://localhost:11434"
default_model = "qwen2.5:7b"

[ai.openai]
api_key = ""
base_url = "https://api.openai.com/v1"
default_model = "gpt-4o-mini"
```

也可以在 Web 设置页面直接修改配置，无需手动编辑文件。

## AI 集成

### Ollama（本地）

```bash
# 安装 Ollama 并拉取模型
ollama pull qwen2.5:7b
```

### OpenAI / 兼容 API

支持所有 OpenAI 兼容 API，例如：

- **OpenAI**: `https://api.openai.com/v1`
- **DeepSeek**: `https://api.deepseek.com/v1`
- **Groq**: `https://api.groq.com/openai/v1`
- **本地代理**: `http://localhost:8080/v1`

在 Web 设置页面填入 API 地址、密钥和模型名称即可。

## 开发

```bash
# Rust 后端
cargo build --release
cargo check -p hyprtrace-daemon
cargo check -p hyprtrace-server

# 前端
cd web
npm install
npm run dev        # 开发服务器 (localhost:5173)
npm run build      # 生产构建
```

## 技术栈

| 组件 | 技术 |
|---|---|
| 守护进程 | Rust, hyprland-rs, rusqlite |
| API 服务器 | Rust, Axum, tokio, reqwest |
| 前端 | React 18, TypeScript, Vite, Tailwind CSS 3, Recharts, Lucide Icons |
| 数据库 | SQLite (WAL mode) |
| AI | Ollama (本地), OpenAI 兼容 API (云端) |

## License

MIT
