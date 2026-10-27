# HyprTrace × DeepSeek Harness (DSH) — AI 助手集成资产

这套文件把 DSH 的 agent loop 变成 HyprTrace 的「时间分析助理」：LLM 路由、人格与信任边界、工具集全部由本目录的 overlay 决定。Rust server 侧负责渲染 overlay、spawn `dsh --profile sdk --patch <overlay>`，并监听一个 Unix socket 接收 dsh 进程内**桥插件**推送的实时事件（token 级流式、状态、错误），同时经该 socket 下发 cancel/models/tools/ping 命令。MCP stdio server 只负责工具面（21 个只读 HyprTrace 工具）。

```
┌────────────────┐  stdio JSON-RPC (dsh SDK)   ┌─────────────────────┐   stdio MCP (NDJSON)   ┌──────────────────────┐   HTTP GET   ┌──────────────────┐
│ hyprtrace-     │ ──────────────────────────> │ dsh --profile sdk    │ ─────────────────────> │ hyprtrace-mcp.mjs    │ ───────────> │ hyprtrace-server │
│ server (Rust)  │ <────────────────────────── │ --patch overlay.yml  │ <───────────────────── │ (node, zero-dep)     │ <─────────── │ /api (loopback)   │
└────────────────┘  session.event / status     └─────────────────────┘   mcp__hyprtrace__*     └──────────────────────┘   JSON/markdown └──────────────────┘
        ▲                    Unix socket (NDJSON, bidirectional)
        └────────────────────── hyprtrace-bridge.mjs（dsh 进程内插件：流式/取消/清单/重连）
```

## 文件清单

| 文件 | 作用 |
|---|---|
| `hyprtrace-mcp.mjs` | 零依赖 Node ESM MCP stdio server，21 个只读工具代理 HyprTrace `/api` |
| `hyprtrace-bridge.mjs` | 零依赖 dsh 进程内 ESM 插件：经 Unix socket 推送 `stream`（含 reasoning delta）/`status`/`agent-error`/`session-created`，接收 `cancel`/`models`/`tools`/`ping` 命令；500ms 退避重连；无 `HYPRTRACE_BRIDGE_SOCK` 时安静禁用 |
| `overlay.example.yml` | **已验证**的 overlay 可读样例，可手工 `dsh --profile sdk --patch` 直接跑 |
| `overlay.template.yml` | 生产模板：Rust 侧 `str::replace` 六个占位符后写入 `$XDG_DATA_HOME/hyprtrace/dsh/overlay.yml` |
| `README.md` | 本文件 |

验证证据（不入库，`.dsh-diag/` 已被 `.gitignore` 忽略）：桥插件 E2E 在 `.dsh-diag/bridge-dev/`（真实 dsh + 隔离 DSH_HOME + Unix socket 监听端 + mock HyprTrace API；流式/取消/清单/重连/禁用路径共 43 项检查），v1 的 MCP 工具面证据保留在 `.dsh-diag/dsh-integration/`。

## 前置条件

- `dsh` 0.2.1-rc.3+（本仓库验证版本），`@deepseek-ai/dsh-mcp-client` 随 dsh 发行
- Node ≥ 18（`hyprtrace-mcp.mjs` 只用 `node:` 内置模块 + 全局 `fetch`；本机验证 Node v26）
- `hyprtrace-server` 可达（默认 `http://127.0.0.1:9420`；生产由 Rust server 把自己的监听地址传进 `HYPRTRACE_API_BASE`）
- 模型路由：dsh 的 `llm-pi-ai` + `opencode-go` provider，API key 由 `OPENCODE_GO_API_KEY` 解析（DSH 凭据机制）

## 快速开始

### A. 手工验证（隔离 DSH_HOME，不碰用户真实 `~/.dsh`）

```bash
cd /path/to/hyprtrace
mkdir -p .dsh-diag/dsh-integration/home/{profiles,sessions}
ln -sf ~/.dsh/.credentials.yaml .dsh-diag/dsh-integration/home/.credentials.yaml
cp -r ~/.dsh/profiles/sdk .dsh-diag/dsh-integration/home/profiles/sdk

# 1) 起一个 mock HyprTrace API（真实数据不要用于测试）
node .dsh-diag/dsh-integration/mock-hyprtrace-api.mjs 19420 &   # token = test-token

export HYPRTRACE_API_BASE=http://127.0.0.1:19420
export HYPRTRACE_AUTH_TOKEN=test-token
export DSH_HOME=$PWD/.dsh-diag/dsh-integration/home

# 2) 用 MCP 客户端自测 server 本身（initialize→tools/list→tools/call）
node .dsh-diag/dsh-integration/mcp-client-test.mjs          # 期望 28/28

# 3) 真实 dsh + 一次中文 prompt（隔离 home，mock API；v1 工具面证据）
node .dsh-diag/dsh-integration/real-dsh-driver.mjs \
  --patch integrations/dsh/overlay.example.yml \
  --out logs/final-frames.ndjson --summary logs/final-summary.json \
  --prompt '列出我今天的应用排行（中文回答，请调用工具查询真实数据，不要编造）。'
```

### A2. 桥插件 E2E（v2：流式 / 取消 / 清单 / 重连）

```bash
cd /path/to/hyprtrace
.dsh-diag/bridge-dev/setup-home.sh            # 隔离 DSH_HOME（拷贝 sdk profile + 凭据符号链接，socket 目录 0700）
cd .dsh-diag/bridge-dev
node render-template-test.mjs                 # 六占位符/渲染/YAML/example 一致性，期望 ALL PASS
node host.mjs                                 # 真实 dsh E2E：39 项检查，期望 39/39
node disabled-test.mjs                        # 无 HYPRTRACE_BRIDGE_SOCK 时安静禁用，期望 4/4
```

`host.mjs` 会自己起 mock HyprTrace API、渲染 `overlay.dev.yml`、监听 Unix socket、spawn 隔离 home 的 dsh，并驱动 `initialize`/`session/prompt`；证据写在 `.dsh-diag/bridge-dev/logs/`（`summary.json`、`checks.txt`、`bridge-wire.ndjson`、`bridge-frames.ndjson`、`dsh-frames.ndjson`、`dsh-stderr.log`）。

### B. 生产（Rust server 侧）

1. 把 `hyprtrace-mcp.mjs`、`hyprtrace-bridge.mjs` 与 `overlay.template.yml` 一起部署到 `$XDG_DATA_HOME/hyprtrace/dsh/`（默认即 `~/.local/share/hyprtrace/dsh/`）。Rust 侧 `resolve_asset` 的搜索顺序是：显式配置路径 → `$HYPRTRACE_DSH_DIR` → 用户 data dir → `/usr/share/hyprtrace/dsh` → 构建时仓库 checkout → 当前目录。
2. 读 `overlay.template.yml`，替换六个占位符（见下），写 `$XDG_DATA_HOME/hyprtrace/dsh/overlay.yml`。
3. 给 dsh 子进程设置：`HYPRTRACE_API_BASE=http://127.0.0.1:<server 端口>`、`HYPRTRACE_AUTH_TOKEN=<config 的 auth_token，可为空>`、**`HYPRTRACE_BRIDGE_SOCK=<HyprTrace 自己监听的 Unix socket 路径>`**。是否设置 **`DSH_HOME`** 见上面的部署注意。socket 路径所在目录应为 0700（契约）；建议先 `listen()` 再 spawn dsh（反过来也能工作：插件每 500ms 重连）。不设置 `HYPRTRACE_BRIDGE_SOCK` = 桥禁用（插件只打一行 stderr，harness 正常启动）。
4. `spawn("dsh", ["--profile", "sdk", "--patch", overlayPath])`（server 侧实现见 `crates/hyprtrace-server/src/ai/dsh.rs`：`initialize {cwd, provider, model}` → `session/resume`（尽力而为）→ `session/prompt`，会话 id 持久化在 data dir 的 `session-id`）。`cwd` 取 `[ai.dsh].cwd`，未配置时用 `$HOME`；助手没有文件工具，cwd 只影响会话 header。
5. 渲染后的 overlay 写到 `$XDG_DATA_HOME/hyprtrace/dsh/overlay.yml`，权限 0600（里面只有 `process.env.…` 引用，不落 token 明文）。
6. server 侧还有一道**工具面守卫**：**每一个** `request/header` 事件的工具表都必须含 `[ai.dsh].tools_prefix`（默认 `mcp__hyprtrace__`）前缀（harness 每轮重建请求时会重发 header；只在首轮检查会漏掉中途挂掉的桥接），否则本轮直接回 error —— MCP 桥坏掉时 dsh 不会启动失败（见坑 13），没有这道守卫模型会没有工具裸答。

> **部署注意（DSH_HOME）**：server 默认**不设置** `DSH_HOME`——dsh 用用户的 `~/.dsh`，这正是凭据（`.credentials.yaml`）与模型路由的来源；`dsh --profile sdk` 首次使用会自动初始化该 profile（bundle = `@deepseek-ai/dsh-base` + `@deepseek-ai/dsh-sdk-app`），不需要手工创建。前提是目标机的 `~/.dsh` 已能正常跑任意 agent（有可用模型路由）。本仓库的隔离 E2E（`.dsh-diag/e2e/setup.sh`）才用 `DSH_HOME` 指向沙箱内拷贝 + `.credentials.yaml` 符号链接，避免碰真实 `~/.dsh`。

## Overlay 文件与占位符契约（六占位符）

`overlay.example.yml` 是模板渲染后的可读样例（本机路径已填好）；`overlay.template.yml` 是 Rust 侧实际使用的模板，只接受下面六个占位符，渲染后不得残留 `{{`：

| 占位符 | 放置位置 | Rust 替换为 |
|---|---|---|
| `{{PROVIDER}}` | `providers:` 下的键（6 空格缩进） | provider 名，如 `opencode-go` |
| `{{MODEL}}` | `models[].id` 与 `models[].name` | 模型 id，如 `deepseek-v4.1-flash` |
| `{{PROVIDER_OPTIONS}}` | 独立一行，8 空格缩进 | 可选 provider 配置行（如 `baseURL: …`、`api: openai-completions`），或空字符串 |
| `{{API_KEY_ENV_LINE}}` | 独立一行，8 空格缩进 | 整行，如 `apiKeyEnv: OPENCODE_GO_API_KEY`，或空字符串 |
| `{{TOOLS_SCRIPT}}` | `args: ['{{TOOLS_SCRIPT}}']` 单引号标量内 | `hyprtrace-mcp.mjs` 的绝对路径；路径含 `'` 时渲染方须转义为 `''` |
| `{{BRIDGE_SCRIPT}}` | `name: 'file://{{BRIDGE_SCRIPT}}'` 单引号标量内 | `hyprtrace-bridge.mjs` 的绝对路径；同样须 `'` → `''` 转义 |

`{{PROVIDER_OPTIONS}}` / `{{API_KEY_ENV_LINE}}` 留空时渲染结果是两行仅有空格的空行，YAML 合法（已测）。模板尾部没有其他双花括号 token。

**`{{PROVIDER_OPTIONS}}` 缩进约定（A 形态，已与 Rust 对齐）**：模板该行自带 8 空格，渲染器提供**首行不带缩进、续行用 `\n` + 8 空格**连接的值（`{{API_KEY_ENV_LINE}}` 同理：值不带缩进）。Rust 侧 `dsh.rs::provider_options_yaml()` 已按此实现，回归测试 `ai::service::tests::provider_options_keep_valid_yaml_indentation` 断言首选项恰好 8 空格、出现 16 空格即失败；本目录的 `bridge-dev/render-template-test.mjs` 用 A 形态渲染 `baseURL` + `api` 两行并断言 YAML 合法、结构正确。

## MCP 协议细节（实测，dsh 0.2.1-rc.3 / `@modelcontextprotocol/client` 2.2.0）

- **transport**：stdio；报文是换行分隔的 JSON-RPC 2.0（一行一个 JSON）。**stdout 只能有 JSON-RPC 帧**，诊断日志全部走 stderr（dsh 的 MCP client 会把 stderr 收进自己的日志）。
- **版本协商**：dsh 的 MCP client 使用 `versionNegotiation: {mode: 'auto'}`，启动时会先用 `server/discover` 探一下 2026-07-28 的「modern wire」。`hyprtrace-mcp.mjs` 对该方法回 `-32601 Method not found`，client 随即判定为 2025 时代 server 并回退到经典 `initialize` 握手。
- **initialize**：client 发 `{protocolVersion, capabilities, clientInfo}`，实测 offer 是 `2025-11-25`；server 回显 offer（支持列表 `2025-11-25 / 2025-06-18 / 2025-03-26 / 2024-11-05`，未知 offer 回最新支持版本），返回
  `{protocolVersion, capabilities:{tools:{listChanged:false}}, serverInfo:{name:'hyprtrace-mcp',version:'1.0.0'}, instructions}`。
  随后 client 发 `notifications/initialized`（无响应）。
- **支持的方法**：`initialize`、`notifications/initialized`、`ping`（回 `{}`）、`tools/list`、`tools/call`；另有 `server/discover` 返回 -32601、未知方法返回 -32601；JSON 解析失败返回 -32700（id 为 null）。
- **工具名**：dsh 的 mcp-client 会把 MCP 原始名映射成稳定 public name `mcp__<serverName>__<rawName>`；本集成 `serverName: hyprtrace`，所以模型看到的是 **`mcp__hyprtrace__<raw>`**（不是 `hyprtrace_<raw>`；最长名字 42 字符，低于 DeepSeek function-name 的 64 上限）。`tools/call` 发给 server 的始终是 raw name。
- **工具结果**：`{content:[{type:'text',text}], isError?}`。JSON 端点返回 pretty JSON（`usage_report` 返回 Markdown 原文）；HTTP 非 2xx、`{"error":…}`、超时、连不上、鉴权失败都以 `isError:true` + 可读文本返回，不会假成功。
- **instructions**：initialize 的 `instructions` 会被 mcp-client 放进 system prompt（实测出现在 persona 之后、信任边界之前），用于重申只读语义与不可信数据边界。
- **进程生命周期**：mcp-client 在正式连接前会先起一个临时探针进程（negotiation 用），所以 server 必须容忍 stdin 立刻关闭并安静退出；`SIGTERM/SIGINT` 直接退出即可。
- **环境变量**：`HYPRTRACE_API_BASE`（默认 `http://127.0.0.1:9420`，尾部 `/` 会被去掉）、`HYPRTRACE_AUTH_TOKEN`（非空时作为 `X-Auth-Token` 发送）、`HYPRTRACE_HTTP_TIMEOUT_MS`（默认 15000）。**dsh 会 scrub 子进程环境里匹配 `/KEY|PASSWORD|SECRET|TOKEN/i` 与 `DSH_*` 的变量**，所以这些值必须在 overlay 的 `env:` 里显式透传（示例已做）；空 token 表示不带鉴权头。

## Bridge 插件协议（`hyprtrace-bridge.mjs`，实测 dsh 0.2.1-rc.3）

插件由 overlay 的 `- insert: - id: hyprtrace-bridge, name: 'file://…/hyprtrace-bridge.mjs'` 加载，运行在 dsh 进程内（**stdout 是 SDK JSON-RPC 通道，插件只写 stderr**）。协议为 NDJSON over Unix socket（`HYPRTRACE_BRIDGE_SOCK`），双向，契约冻结在 `.dsh-diag/deep-fusion/CONTRACT.md` §1。

插件 → HyprTrace：

```
{"type":"hello","pid":123,"protocol":1}                      连接建立（每次重连都重发，总是首帧）
{"type":"stream","sessionId":"…","frame":{…}}                agent/assistant-stream 原样 frame
{"type":"status","sessionId":"…","status":"running"|"idle"}  agent/status
{"type":"agent-error","sessionId":"…","message":"…","code":"…"} agent/error
{"type":"session-created","sessionId":"…","parentSession":"…"}
{"type":"reply","id":"…","ok":true,"models":{"opencode-go":[{"id","name"}]}}
{"type":"reply","id":"…","ok":true,"tools":[{"name","description"}]}
{"type":"reply","id":"…","ok":true,"cancelled":true|false}
{"type":"reply","id":"…","ok":false,"error":"…"}
```

HyprTrace → 插件：`{"cmd":"cancel","id","sessionId"}`、`{"cmd":"models","id","providers":["opencode-go"]}`、`{"cmd":"tools","id","sessionId?"}`、`{"cmd":"ping","id"}`。

- **hello**：`protocol: 1`；`pid` 是 dsh 进程 pid（可与 spawn 的 child pid 对账）。
- **重连**：socket 断开后固定 **500ms** 重试（实测监听端重启后 502–503ms 收到新 hello）；每次连接先发 hello，再 flush 断线前排队的事件；断线时丢弃旧队列防止重放。
- **流式**：`frame` 是 dsh 的 `agent/assistant-stream` 帧原样转发，`frame.type ∈ start|chunk|end`，chunk 里 `text-delta` / `reasoning-delta` 逐步到达（实测单轮 74–283 个 text-delta、27–312 个 reasoning-delta，text-delta 跨度 1.8–3.6s），`end.outcome.kind = committed` 表示本轮提交。
- **取消**：`cancel` 按 `ctx.get('agents').list()` 匹配 `String(agent.session.id)`，调用 `agent.cancel({kind:'user'},{keepInbox:true})`。找到 → `{"ok":true,"cancelled":true}`（实测 cancel reply 1ms 内返回，随后 SDK `turn/end reason={kind:'aborted',reason:{kind:'user'}}`）；找不到 live agent → `{"ok":true,"cancelled":false}`（HyprTrace 映射 `/api/ai/cancel` 的 `not_found`）；异常 → `ok:false`。
- **models**：逐 provider `ctx.get('llm').listModels(provider)`，返回 `{id,name}`（未知/未挂载 provider → `[]`，不是错误）。插件激活有 0–250ms 竞态，命令对空结果做最多 750ms 有界重试。
- **tools**：`ctx.get('tools').schemas(agent?.ctx)`，只回 `{name,description}`。带 `sessionId` 且该 session 有 live agent 时用 agent scope；否则（未传或 agent 不存在）回全局工具视图并做同样的有界重试。实测 dsh 启动后 `hello` 即调用也能拿到 21 个 `mcp__hyprtrace__*` 工具。
- **ping**：立即回 `{"ok":true}`。
- **禁用**：未设置 `HYPRTRACE_BRIDGE_SOCK` 时插件只打印 `HYPRTRACE_BRIDGE_SOCK not set; bridge disabled`，不建连接、不影响 harness。

## 工具清单（21 个，全部只读 GET）

| raw name（模型侧加 `mcp__hyprtrace__` 前缀） | 端点 | 参数（默认 → 钳制） |
|---|---|---|
| `today_summary` | `/api/summary` | `date`（默认服务器本地今天） |
| `app_ranking` | `/api/apps` | `from`/`to`（默认今天）、`limit`（10 → 1..500） |
| `sessions` | `/api/sessions` | `from`/`to`（默认今天）、`page`（1 → 1..500）、`per_page`（50 → 1..500）、`class` |
| `timeline` | `/api/timeline` | `date` |
| `search_titles` | `/api/search` | `q`（必填，子串）、`from`/`to`（缺省=全历史）、`limit`（50 → 1..200） |
| `title_stats` | `/api/titles` | `from`（今天-6）/`to`（今天）、`class`、`limit`（50 → 1..500） |
| `efficiency` | `/api/efficiency` | `date` |
| `predict` | `/api/predict` | `window`（14 → 3..30） |
| `goals` | `/api/goals` | — |
| `categories` | `/api/categories` | — |
| `projects` | `/api/projects` | — |
| `project_stats` | `/api/projects/stats` | `from`/`to`（默认今天） |
| `current_status` | `/api/status` | — |
| `insights_transitions` | `/api/insights/transitions` | `from`/`to`（默认最近 7 天）、`min_ms`（3000 → 0..3600000）、`limit`（12 → 1..100） |
| `insights_rhythm` | `/api/insights/rhythm` | `from`/`to`（默认最近 7 天） |
| `insights_fragmentation` | `/api/insights/fragmentation` | `from`/`to` |
| `insights_anomalies` | `/api/insights/anomalies` | `from`/`to`、`window`（14 → 1..90）、`threshold`（2.0 → 0.5..5.0） |
| `insights_forecast` | `/api/insights/forecast` | `class`、`days`（30 → 1..365）、`horizon`（7 → 0..365）；无 from/to |
| `insights_cooccurrence` | `/api/insights/cooccurrence` | `from`/`to`、`window_minutes`（30 → 1..1440）、`limit`（10 → 1..100） |
| `insights_disruption_impact` | `/api/insights/disruption-impact` | `from`/`to` |
| `usage_report` | `/api/report` | `from`（今天-6）/`to`（今天）；返回 Markdown 文本 |

参数名、默认值、钳制与 `crates/hyprtrace-server/src/routes/{data,insights}.rs` 一致；MCP server 先按同一规则钳一次，HTTP 端再钳一次。没有暴露任何 DELETE/PUT 端点，写能力为零。

## 禁用的 overlay row id 清单（33 行，实测 `request/header` 只剩 MCP 工具）

这些 id 来自 `@deepseek-ai/dsh-base` 的 insert 列表（`/usr/lib/node_modules/@deepseek-ai/dsh/node_modules/@deepseek-ai/dsh-base/cordis.patch.yml`）与 `@deepseek-ai/dsh-sdk-app` 的 patch。分组和它们各自会引入的东西：

**工具行（不禁止就会出现在模型工具表里）**

| row id | 不禁用会暴露的工具（实测名） |
|---|---|
| `tool-bash` | `bash` |
| `tool-pwsh` | `pwsh`（非 Windows 上本已禁用，仍显式写） |
| `tool-jobs` | `job_list` / `job_output` / `job_kill` |
| `tool-fs` | `read` / `write` / `edit` / `read_image` |
| `tool-fs-search` | `glob` / `grep` |
| `tool-working-directory` | `working_directory` |
| `tool-todo` | `todo_write` |
| `tool-goal` | `create_goal` / `get_goal` / `update_goal` |
| `tool-web` | `web_search` / `web_fetch`（+ `web`/`web-search-deepseek`/`web-deepseek-api`/`web-fetch-http` 提供实现） |
| `tool-skill` | `skill` |
| `tool-workflow` | `workflow`（+ `ptc-runtime`/`workflow-ptc`） |
| `tool-subagent` | `subagent` |
| `tool-subagent-fork` | `subagent_fork` |
| `tool-subagent-control` | `send_message` / `interrupt_agent` |
| `tool-subagent-list-agents` | `list_agents` |
| `plan-mode` | `exit_plan_mode` + plan-mode 提示段 |

**prompt / 工作区指令行（不禁止就会污染 system prompt）**

| row id | 不禁用会注入的内容 |
|---|---|
| `agent-instructions` | 从 cwd 向上找 `AGENTS.md` 并注入（本仓库会注入整个 HyprTrace 约定） |
| `skill` / `skill-filesystem` | 仓库 `.dsh/skills/` 的技能目录与加载机制 |
| `repeat-tool-reminder` | 连续调用工具的提醒段 |
| `goal` / `goal-round-driver` / `command-goal` / `command-feedback` | goal 持久化与 slash 命令（无 UI 的 SDK profile 不需要） |

**网络 / 工作流 / MCP 资源机制**

`web`、`web-search-deepseek`、`web-deepseek-api`、`web-fetch-http`、`ptc-runtime`、`workflow-ptc`、`mcp-resources`（会额外注册 `list_mcp_resources` / `read_mcp_resource` 等工具）。

**SDK bundle 插入行**

`workspace-dependencies`、`skill-office`（`@deepseek-ai/dsh-sdk-app` 的 insert；默认条件禁用，这里显式禁用）。

> 另外 `@deepseek-ai/dsh-sdk-app` 自己会禁用 `hmr` 与 `session-title-llm`，不需要我们重复。`tools` / `system-prompt` / `mcp-client` 行保留；只 patch `system-prompt` 的 `personaPrefix` 与 `personaSuffix`（loader patch 是**整段 config 替换**，其余字段走插件默认值）。

## 验证步骤与判据

1. **MCP server 自测**：
   ```bash
   node .dsh-diag/dsh-integration/mcp-client-test.mjs
   ```
   期望 `28/28 checks passed`。覆盖：`server/discover` 回退信号、initialize 版本回显、ping、21 个工具名、read-only 注解、`today_summary` 数据、limit/page/window/threshold 钳制、Markdown 端点、Markdown 端点上的 HTTP-200+`{"error"}` 约定、缺参/未知工具/HTTP 401 错误路径、-32700。原始报文见 `.dsh-diag/dsh-integration/logs/mcp-selftest-frames.ndjson`。

2. **模板渲染（六占位符）**：
   ```bash
   node .dsh-diag/bridge-dev/render-template-test.mjs
   ```
   期望 `ALL PASS`（26 项）：六个占位符替换后无 `{{` 残留、YAML 可解析（默认/bare/A 形态 provider options 三种渲染）、`'`→`''` 转义、bridge/mcp insert 行与脚本路径、与 `overlay.example.yml` 结构一致。

3. **桥插件 E2E（真实 dsh，隔离 DSH_HOME + mock API）**：
   ```bash
   .dsh-diag/bridge-dev/setup-home.sh
   cd .dsh-diag/bridge-dev && node host.mjs
   ```
   期望 `39/39 checks passed`。判据：
   - `hello` 的 `protocol=1`、`pid` 与 spawn 的 dsh 子进程一致；
   - `models` 返回 `opencode-go` 的 `deepseek-v4.1-flash`；`tools` 返回 21 个、全部 `mcp__hyprtrace__*`；
   - 长回答 prompt：`text-delta` ≥5 且时间跨度 ≥500ms（实测 74–283 个、1.8–3.6s），`reasoning-delta` ≥1（实测 27–312 个），流有 start/chunk/end 且 end 为 committed；
   - 流进行中发 `cancel`：reply `ok+cancelled=true`，`turn/end` 为 `{kind:'aborted',reason:{kind:'user'}}`，随后 `status=idle`；
   - 监听端重启：≤10s 内收到新 hello（实测 502–503ms），命令与流式恢复；
   - 负路径：未知命令 `ok:false`、未知 session 的 cancel `cancelled:false`、坏 JSON 行不断连接；
   - dsh stdout 0 条非 JSON、socket 0 条坏行、dsh 退出码 0、插件日志只在 stderr。
   证据：`.dsh-diag/bridge-dev/logs/{summary.json,checks.txt,bridge-wire.ndjson,bridge-frames.ndjson,dsh-frames.ndjson,dsh-stderr.log}`。

4. **桥禁用路径**：
   ```bash
   cd .dsh-diag/bridge-dev && node disabled-test.mjs
   ```
   期望 `4/4 checks passed`：无 `HYPRTRACE_BRIDGE_SOCK` 时插件打印 opt-out 行、dsh 正常 initialize、无连接尝试、stdout 纯 JSON-RPC。

5. **MCP 工具面（v1 证据，回归用）**：
   ```bash
   node .dsh-diag/dsh-integration/mcp-client-test.mjs           # 28/28
   node .dsh-diag/dsh-integration/real-dsh-driver.mjs --patch integrations/dsh/overlay.example.yml \
     --out /tmp/frames.ndjson --summary /tmp/summary.json
   ```
   判据：`session.event.request/header` 的 `tools` 恰好 21 个 `mcp__hyprtrace__*`，`tool/call`/`tool/result` 出现，mock API 收到带 `x-auth-token` 的 `/api/*`，`turn/end reason=completed`。证据在 `.dsh-diag/dsh-integration/logs/final-*`（v1 录制的证据，脚本与路径仍可复跑）。

6. **system prompt 判据**：`.dsh-diag/dsh-integration/logs/final-system-prompt.txt` 应只有：DSH identity 一行 + HyprTrace persona + MCP instructions + TRUST BOUNDARY 段；**没有** "coding agent"、AGENTS.md、skill 或 plan-mode 文本。

## 已知坑 / 限制

1. **`--patch` 是整段 config 替换**：`- id: system-prompt` 覆盖后只保留你写的 `personaPrefix`/`personaSuffix`，其余字段回到插件默认；不要期望 patch 与已有 config 合并。
2. **`agent-instructions` 必须先禁用**：否则 dsh 会从 cwd 向上读 `AGENTS.md`，把整个编码 agent 人格和仓库约定塞进 system prompt（stage A 实测确实发生）。
3. **环境变量会被 scrub**：dsh 的 MCP client 起子进程时丢弃环境里匹配 `KEY|PASSWORD|SECRET|TOKEN` 的名字和所有 `DSH_*`，所以 `HYPRTRACE_AUTH_TOKEN` / `HYPRTRACE_API_BASE` 必须在 overlay 的 `env:` 中显式列出；否则 token 为空 → 服务端开了鉴权时工具调用会返回 401 工具错误。
4. **MCP 探针进程**：client 会先起临时进程做版本协商，再起正式进程；server 不能假设自己只会被连接一次，必须能承受 stdin 立即 EOF、且启动阶段不往 stdout 写任何东西。
5. **工具结果不会无条件裁剪**：dsh 的 `tool-result-pruner` 只在压缩触发时把 >8192 字符的 tool result 修成 head 4096 + tail 1024；上下文不紧张时原样通过。`hyprtrace-mcp.mjs` 自己对 >60000 字符的结果做显式截断并附注，但大查询（`sessions per_page=500`、长报告）仍应主动收窄，别把窗口塞满。
6. **工具名长度**：public name 有 64 字符上限，本集成最长 42；若改 `serverName`/raw 名要重新算（超限会触发 mcp-client 的哈希归一化，名字会变）。
7. **`DSH_HOME` 决定 profile 与凭据的来源**：不设会落到用户 `~/.dsh`（要求那里已有 `profiles/sdk`，且凭据可解析；与用户自己的 harness 共享 sessions/profile）。隔离 home 需要 `profiles/sdk` 与可用的 `.credentials.yaml`（验证时用符号链接）。Rust 侧 spawn 时是否设置由部署决定。
8. **`literal` 路径**：`{{TOOLS_SCRIPT}}` 在 YAML 单引号标量里，路径含 `'` 时必须 `''` 转义（Rust 侧约定）；路径含空格没问题。若部署路径变化，别忘了重新渲染 overlay。
9. **只读边界靠工程保证**：MCP server 只实现 GET 代理，没有 DELETE/PUT；persona 与 instructions 都写明只读。将来若要加写工具，必须重新评估信任边界（窗口标题是不可信输入）。
10. **模型行为**：模型可能先输出英文的 plan text 再给中文答案（deepseek-v4.1-flash 的 reasoning 呈现），也可能不调工具直接答——persona 里已要求"never guess"，必要时在 prompt 里明确"请调用工具查真实数据"。
11. **不要在真实实例上跑验证脚本**：验证全部打 mock API；真实 9420 只读访问也建议由产品路径打（带 token 的 GET），不要写。
12. **MCP server 的 stdout 一旦被污染（例如误用 `console.log`）整个连接会坏**；调试只能用 `console.error`/`process.stderr`。
13. **MCP 启动失败不会中止 dsh**：即使 overlay 里写了 `failOnStartupError: true`（模板已写），mcp-client 的连接/发现失败只会在 dsh stderr 记一条错误、harness 照常启动，只是 tool registry 里没有 MCP 工具（实测）。真正的快速失败在 HyprTrace 侧：`ai::dsh` 检查**每一个** `request/header.tools` 是否含 `[ai.dsh].tools_prefix`（默认 `mcp__hyprtrace__`）前缀，没有就回 `HyprTrace tools are missing inside the harness …` 并丢弃子进程（有单测 + `.dsh-diag/e2e/startup-failure-test.sh` 验证）。
14. **turn 进行中子进程死亡**：HyprTrace 侧靠 dsh stdout EOF 的 `watch` 立即发现并回 `DSH: the harness process exited mid-turn`（不会等满超时预算）；`turn/end` 后的 idle 状态最多等 5s 宽限。
15. **桥插件与 MCP server 的 stdout 纪律相同**：`hyprtrace-bridge.mjs` 运行在 dsh **主进程**里，任何 `console.log`/`process.stdout.write` 都会污染 SDK JSON-RPC 通道；只能写 stderr。E2E 会统计 stdout 非 JSON 行数（应为 0）。
16. **桥的 socket 由 HyprTrace 侧监听**：路径放 0700 目录（契约）；建议先 listen 再 spawn dsh，但插件有 500ms 退避重连，顺序反过来也能自愈。`HYPRTRACE_BRIDGE_SOCK` 不设置 = 桥禁用（便于灰度/排障）。
17. **桥 socket 断开 ≠ 模型停止**：run 中断线只意味着 HyprTrace 暂时收不到 `stream`/`status`；dsh 的 turn 会继续跑完。需要停 turn 必须通过 `cancel` 命令（或直接杀 dsh 子进程）。
18. **cancel 语义**：`cancel` 是"尽力而为"——找不到 live agent 时回 `cancelled:false`（不是错误）；turn 恰好已结束时可能 cancel 成功但已经 `completed`，HyprTrace 侧应以自己收到的 `turn/end`/`cancelled` 事件为准，不要假设 cancel 后必然 aborted。
19. **插件激活竞态**：`hello` 到达时 dsh 的 MCP tool registry / pi-ai 模型路由可能还在激活（实测 0–250ms），桥的 `tools`/`models` 命令对空结果做 ≤750ms 有界重试；HyprTrace 侧若仍收到空清单，建议重试一次而不是报错。
20. **桥事件量**：一轮回答可产生数百个 `stream` 帧（含 reasoning delta），HyprTrace 必须按行解析并做背压/丢弃策略；插件在未连接时只缓存最近 1024 帧，断线重连会丢弃旧帧。
