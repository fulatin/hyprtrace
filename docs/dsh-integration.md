# HyprTrace × DeepSeek Harness（DSH）深度融合

HyprTrace 的 **AI 部分整体**交给本机的 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)（`dsh`）：
`hyprtrace-server` 里不再有自己的 LLM provider、agent 循环或工具执行器。AI 聊天、
非流式问答、周报生成、主动提醒全部由同一个常驻的 `dsh` 子进程驱动；模型路由、
工具调用、会话历史、上下文压缩、取消都由 harness 负责。

```
web AIChat ──NDJSON──▶ /api/ai/* ──▶ AiService ──▶ dsh 子进程（agent / 模型 / 历史 / 工具）
                                        ▲   │            │
                                        │   └── SDK stdio JSON-RPC（prompt、事件、resume）
                                        └────── Unix socket ── hyprtrace-bridge 插件
                                                              （token 流、推理、取消、清单）
```

## 为什么这样接

两边没有可共享的代码层（Rust/Axum vs TS/cordis），但 DSH 提供了三个稳定边界，全部用上了：

| 边界 | 用途 |
|---|---|
| SDK stdio JSON-RPC（`initialize` / `session/prompt` / `session/resume` / `session.event` / `session.status`） | 驱动 agent、拿到工具调用与回合结果 |
| DSH 插件（`integrations/dsh/hyprtrace-bridge.mjs`，profile patch 以 `file://` 加载） | 官方 SDK 不转发的东西：token 级流式、reasoning delta、主动取消、模型/工具清单 |
| MCP stdio（`integrations/dsh/hyprtrace-mcp.mjs`） | 模型可用的 21 个只读 HyprTrace 数据工具 |

## 目录

| 文件 | 作用 |
|---|---|
| `crates/hyprtrace-server/src/ai/service.rs` | `AiService`：子进程生命周期、会话、流式事件、取消、清单、健康状态 |
| `crates/hyprtrace-server/src/ai/bridge.rs` | Unix socket 桥：NDJSON 帧、命令/应答关联、连接跟踪 |
| `crates/hyprtrace-server/src/routes/ai.rs` | HTTP 端点（NDJSON 流、取消、状态、清单、周报、历史） |
| `integrations/dsh/hyprtrace-bridge.mjs` | harness 内插件：转发 stream/status/error、处理 cancel/models/tools/ping |
| `integrations/dsh/hyprtrace-mcp.mjs` | 21 个只读数据工具的 MCP server |
| `integrations/dsh/overlay.template.yml` | profile patch 模板：模型路由、助理人设+信任边界、禁用的编码工具、MCP/bridge 接入 |
| `integrations/dsh/overlay.example.yml` | 渲染后的可读样例 |

> 旧的自研 AI 栈（`ai/ollama.rs`、`ai/openai.rs`、`ai/tools.rs`、`AiManager`、`run_agent`、
> `/api/ai/chat/stream*`）已删除。`[ai.ollama]`、`[ai.openai]` 等旧配置键会被安全忽略。

## 前置条件

1. `dsh` CLI 在 `PATH`（`dsh --version`），Node ≥ 18。
2. `~/.dsh` 里至少有一个可用的模型路由：默认 `opencode-go` / `deepseek-v4.1-flash`
   （凭据引用 `OPENCODE_GO_API_KEY`）。用别的路由就改 `[ai.dsh] provider/model/api_key_env`。
3. `dsh --profile sdk` 首次使用会自动初始化 profile，不需要手工创建。

## 配置

```toml
[ai]
proactive_interval_minutes = 120        # 主动提醒的间隔（分钟）

[ai.dsh]
enabled = true
command = "dsh"                          # 可执行文件（走 PATH；systemd 下建议绝对路径）
profile = "sdk"
provider = "opencode-go"                 # DSH 里已配置的 provider id
model = "deepseek-v4.1-flash"
api_key_env = "OPENCODE_GO_API_KEY"
request_timeout_seconds = 300            # 单轮预算；超时会丢弃 harness 子进程

# 可选/高级：
# cwd = "~/git"                         # 会话 header 的工作目录（助手没有文件工具）
# base_url = "https://…"                # 覆盖 provider 的 baseURL
# api = "openai-completions"            # 覆盖 pi-ai api 类型
# tools_script / bridge_script / overlay_template = "/abs/path"  # 资产位置覆盖
# tools_prefix = "mcp__hyprtrace__"     # 工具面守卫前缀；空字符串关闭守卫
# extra_args = []                       # 追加到 dsh 命令行的参数
```

设置页（Settings → AI）读写的就是这一段；`PUT /api/config` 合并写入 TOML，保留未知键。
`GET /api/config` 返回 `{ai: {proactive_interval_minutes, dsh: {...}}, ai_status: {...}}`。

## HTTP 端点

| 端点 | 说明 |
|---|---|
| `GET /api/ai/models[?refresh=1]` | `{providers:{dsh:[...]}, default:"dsh", current:{provider,model}, status}`；不启动 harness（`refresh=1` 才启动并取真实清单） |
| `GET /api/ai/status` | `{enabled, ready, provider, model, bridge_connected, tool_count, error}` |
| `GET /api/ai/tools` | harness 实际提供给模型的工具表（`{tools:[{name,description}]}`） |
| `POST /api/ai/chat/agent` | NDJSON 流式聊天（见下）；客户端断连 = 取消该轮 |
| `POST /api/ai/cancel` | `{conversation_id}` → `{status:"ok"|"not_found"}` |
| `POST /api/ai/chat` | 非流式问答 → `{reply, model:"dsh"}` |
| `POST /api/ai/report/weekly` | 生成 Markdown 周报 → `{report}` |
| `GET/DELETE /api/ai/conversations` | 展示用历史（写自聊天轮）/ 清空并取消所有在跑回合 |

聊天请求：`{message, conversation_id?, model?}`。

- `conversation_id` 直接用作 DSH session id：不同对话互不串台；缺失时用服务端默认会话。
- `model` 与当前运行时模型不同时，后端会重启 harness 子进程并以该模型 `initialize`，
  然后 `session/resume` 回到该对话（因此模型下拉是真实生效的）。

NDJSON 事件（`application/x-ndjson`，一行一个）：

```json
{"type":"reasoning","delta":"..."}                      // 思考增量（来自 bridge）
{"type":"text","delta":"..."}                           // 回答增量（来自 bridge）
{"type":"tool_call","id":"...","name":"mcp__hyprtrace__…","args":{...}}
{"type":"tool_result","id":"...","name":"...","ok":true,"result":...}
{"type":"cancelled"}                                     // 终止事件（取消）
{"type":"done"}                                          // 终止事件（正常结束）
{"type":"error","message":"..."}                         // 终止事件（失败）
```

## 会话、取消与容错

- **会话**：`conversation_id` 即 DSH session id；服务端持久化一个默认会话 id 供旧客户端使用。
  子进程重启（崩溃/模型切换/配置变更）后，下次该对话的第一轮会 `session/resume`。
- **取消**：`POST /api/ai/cancel` 或客户端断开连接 → 通过 bridge 调
  `agent.cancel({kind:'user'},{keepInbox:true})`，DSH 以 `turn/end reason=aborted` 结束，流以
  `cancelled` 终止（不再有 `done`）。
- **子进程死亡**：stdout EOF 通过 `watch` 立即发现 → 本轮回 `error`，下一轮自动重建；
  不会干等到 `request_timeout_seconds`。
- **超时**：SDK 没有 cancel 方法，超时后直接丢弃整个子进程并回 `error`。
- **工具面守卫**：MCP 桥起不来时 dsh 不会启动失败（插件错误只写日志），模型会裸答。
  因此**每个** `request/header` 事件的工具表都必须含 `[ai.dsh].tools_prefix`
  （默认 `mcp__hyprtrace__`）前缀，否则本轮直接回 error 并丢弃子进程。
- **并发**：不同 conversation 的回合可以并行（事件按 session id 路由）；bridge 未连接时
  取消/流式降级（文本回退到 `assistant/message`，取消不可用）。

## 安全边界

- MCP 工具**全部只读**（21 个 GET 代理），编码工具（bash/fs/web/subagent/…）在 overlay 里全部禁用；
  模型实际只看到 `mcp__hyprtrace__*`。
- 人设保留信任边界：窗口标题、应用 class、通知文本、DB 值都是不可信数据。
- token 只经环境变量传给 dsh/插件；运行期 overlay 落盘 0600 且不含明文凭据；
  bridge socket 位于 0700 目录、0600 权限。
- dsh 与插件以当前用户身份运行，不额外提权；HyprTrace API 的鉴权/Origin 白名单照旧生效。

## 部署

`cargo run` / 开发检出：资产直接从仓库 `integrations/dsh/` 解析，无需额外步骤。

安装版：把 `integrations/dsh/` 下的三个脚本 + 模板拷到 `$XDG_DATA_HOME/hyprtrace/dsh/`
（`~/.local/share/hyprtrace/dsh/`）；`resolve_asset` 搜索顺序：
显式配置 → `$HYPRTRACE_DSH_DIR` → 用户 data dir → `/usr/share/hyprtrace/dsh` →
构建时仓库路径 → 当前目录 `integrations/dsh`。

服务作为 systemd 用户服务运行时 `PATH` 可能不含 `dsh` 的安装目录，`[ai.dsh].command`
建议写绝对路径。

## 冷启动

harness 是**懒启动**的：server 刚启动时 `/api/ai/status.ready=false`、`bridge_connected=false`，
前端显示 "DeepSeek Harness is idle"（可发送，不阻塞）。第一条消息会 spawn `dsh` 子进程
（约 1–2s），该轮结束后状态变 `ready=true`。设置页的 **Test connection**（`GET /api/ai/models?refresh=1`）
可提前拉起并验证路由，适合排障。

## 排障

| 现象 | 排查 |
|---|---|
| `AI is disabled` | `[ai.dsh] enabled = true` 后重启 server（或设置页打开） |
| `failed to spawn dsh` | `dsh` 不在 PATH（systemd 环境）→ `command` 写绝对路径 |
| `DSH initialize failed: no adapter registered for provider …` | `[ai.dsh].provider` 与 DSH 里配置的 provider 不一致 |
| `Insufficient Balance` / `401` / `ACCOUNT_SIGN_IN_REQUIRED` | DSH 侧模型路由/凭据问题；先确认同一个 provider/model 在 DSH GUI 或 `dsh --profile sdk` 能跑 |
| `HyprTrace tools are missing inside the harness` | MCP 桥没起来：`node` 在 PATH？资产路径对？手动运行 `node hyprtrace-mcp.mjs` 看 stderr |
| 没有 token 级流式、只有整段文本 | bridge 未连上（`/api/ai/status.bridge_connected=false`）；检查 `{{BRIDGE_SCRIPT}}` 渲染与 `HYPRTRACE_BRIDGE_SOCK` |
| 想看 harness 日志 | `RUST_LOG=hyprtrace_server=debug`，日志里以 `dsh:` 前缀的 stderr 会转出 |

## 验证

```bash
cargo test --workspace                 # daemon 54 / server 103（含 ai::bridge 3、ai::service 15）
# 隔离实例（DB 快照 + 独立 HOME + 沙箱 DSH_HOME），不碰运行中的 9420：
.dsh-diag/e2e/setup.sh
env HOME=$PWD/.dsh-diag/e2e/iso/home XDG_CONFIG_HOME=$PWD/.dsh-diag/e2e/iso/config \
    DSH_HOME=$PWD/.dsh-diag/e2e/dsh-home RUST_LOG=hyprtrace_server=debug,info \
    ./target/debug/hyprtrace-server
python3 .dsh-diag/e2e/chat.py "今天我用得最多的应用？"     # 观察 reasoning/text/tool_call/done
python3 .dsh-diag/deep-fusion/test-cancel.py               # 显式取消 + 客户端 abort
```

`.dsh-diag/` 是 gitignore 的本地诊断目录，不随仓库分发。

## 已知限制 / 后续

- **多会话 UI 尚未做**：后端支持按 `conversation_id` 隔离 DSH 会话，但聊天历史仍是单条滚动
  记录（"新对话"= 清空历史 + 换新 id）。会话列表/切换是下一期工作。
- **模型清单只有 id/name**（bridge 契约未暴露 contextWindow 等）；模型切换会重启子进程（约 1–2s）。
- **写操作未暴露**：`set_goal`/`delete_goal`/`send_reminder` 没做成 MCP 工具（只读助手）。
- **bridge 是 DSH 0.2.x 的插件事件契约**：`agent/assistant-stream` 等属于 harness 插件面，
  升级 DSH 后需要复核 `integrations/dsh/README.md` 里的验证脚本。
- 旧的 `[ai.ollama]`/`[ai.openai]` 配置键不会自动清理，只是被忽略。
