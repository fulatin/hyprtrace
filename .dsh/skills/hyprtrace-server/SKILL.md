---
name: hyprtrace-server
description: hyprtrace-server 参考：全部 /api 端点与其参数钳制、SQL 层与日期列约定、每个派生指标的确切公式（效率分、趋势预测、洞察七件套）、鉴权与 Origin 白名单、三个后台任务、DeepSeek Harness 运行时与 21 个只读 MCP 工具、以及会踩坑的不变量。
whenToUse: 新增或修改 REST 端点、改派生指标/统计算法、排查接口数据不对、处理鉴权/CORS、改 AI 集成（DSH 运行时/MCP 工具）、改配置读写时。
---

# hyprtrace-server 参考

Axum 0.7 + tokio，binary-only（无 `lib.rs`，测试全是内联单测）。`Arc<AppState>{ db: tokio::Mutex<Database>, config: tokio::Mutex<Config>, ai: Arc<ai::AiService> }`，`Database` 内部是**一个 rusqlite 连接**（无连接池，也不套 `spawn_blocking`）。

## 一、HTTP 表面

**约定**：多数 handler 是 `Result<Json<T>, Json<Value>>`，失败也返回 **HTTP 200 + `{"error": ...}`**。只有 `PUT /api/projects`（400/500）与 `/api/ai/chat/agent`（400，空消息/未启用）会返回真实状态码。未匹配的 `/api/*` → 404 JSON；其余路径落到 SPA fallback（200 + index.html，故意用 `.fallback()` 而不是 `.not_found_service()`，否则前端路由刷新会 404）。例外：`/assets/*` 由独立 `ServeDir` 提供，缺失文件返回 **404**（不能回退 index.html，否则懒加载 chunk 会以 MIME 错误失败）。静态缓存：`/assets/*` = `public, max-age=31536000, immutable`，index.html 等 = `no-cache`（`main.rs::cache_control_for_path`，有单测）。

数据端点（`routes/data.rs`）：

| 端点 | 关键参数（默认 → 钳制） | 返回 |
|---|---|---|
| `GET /api/health` | — | `{status,version}`（version 硬编码） |
| `GET /api/summary` | `date`(本地今天) | `TodaySummary` |
| `GET /api/apps` | from/to, `limit`(10 → 1..500，两层都钳) | `AppRank[]` |
| `GET /api/timeline` | `date` | `HourlyBucket[24]` |
| `GET /api/sessions` | from/to, `page`(1), `per_page`(50 → 1..500，**两层都钳**), `class` | `{data,total,page,per_page}` |
| `DELETE /api/sessions` | from/to, `class` | `{deleted,rebuilt_summaries}` |
| `GET /api/app/:class/trend` | from/to, `granularity=hour` | `DailyTrend[]` |
| `GET /api/apps/classes` / `GET /api/apps/metadata` | from/to；`classes=a,b` | `string[]` / `{entries}`（不查库，走 desktop 缓存） |
| `GET /api/activity/events` / `daily` | `limit`(100 → 1..1000) / `days`(371 → **7..730**) | 事件列表 / 零填充的日均序列 |
| `GET /api/titles` | from(今天-6)/to, `class`, `limit`(50 → 1..500) | `TitleStat[]` |
| `GET /api/search` | `q`（必填，**子串**匹配）、from/to（可省略＝**全历史**）、`limit`(50 → 1..200，两层都钳) | `SearchHit[]`：会话字段内联 + `snippet` |
| `GET /api/resources` / `disruptions` | from/to, `limit`（**只在 handler 钳制**） | `AppResource[]` / `DisruptionEvent[]` |
| `GET /api/efficiency` | `date` | `EfficiencyScore` |
| `GET/PUT /api/goals` | PUT = **整体替换**（`replace_all=true`） | `{goals,progress}` |
| `GET /api/predict` | `window`(14 → 3..30) | `TrendPrediction` |
| `GET /api/status` | — | waybar 用的当前状态 |
| `GET /api/workspace/recommendations` | `days`(14, **未钳制**) | `WorkspaceRecommendation[]` |
| `GET/PUT /api/categories`、`/api/projects`、`GET /api/projects/stats` | PUT projects 重名 → 400，并**回显重排后的 id/sort_order** | 见类型定义 |
| `GET /api/report` | from/to | **markdown 下载**（非 JSON） |

洞察端点（`routes/insights.rs`，薄 handler，只做参数钳制，公式在 `insights.rs`）：

| 端点 | 参数（默认 → 钳制） |
|---|---|
| `transitions` | from/to, `min_ms`(3000 → 0..3_600_000), `limit`(12 → 1..100) |
| `forecast` | `class`(空=总体), `days`(30 → 1..365), `horizon`(7 → 0..365)。**没有 from/to 参数**，`to` 恒为本地今天 |
| `rhythm` / `fragmentation` / `disruption-impact` | from/to |
| `cooccurrence` | from/to, `window_minutes`(30 → 1..1440), `limit`(10 → 1..100) |
| `anomalies` | from/to, `window`(14 → 1..90), `threshold`(2.0 → 0.5..5.0) |

配置：`GET /api/config`（**永不返回 API key**）；`PUT /api/config` 走 toml_edit 读改写（保留未知键与注释），更新 `[ai.dsh]` 后会丢弃并懒重启 harness（`AiService::reconfigure`）。**`auth_token` 不可通过 API 修改**，只能编辑 config.toml；`weekly_report_day` 无校验，写 >7 会静默关闭周报。

AI：`/api/ai/models[?refresh=1]`（DSH 实际模型清单 + status）、`/status`（enabled/ready/bridge_connected/tool_count/error）、`/tools`（harness 真实工具表）、`/chat/agent`（**NDJSON**，事件七种：`text` / `reasoning` / `tool_call` / `tool_result` / `done` / `cancelled` / `error`）、`/cancel`、`/chat`（非流式）、`/report/weekly`、`/conversations`（GET/DELETE）。**已删除** `/chat/stream`、`/chat/stream/text`。

## 二、SQL 层（`db/`）

`db.rs` 已拆成模块（无行为变化）：`mod.rs`（`struct Database` + `open` + `mod` 声明）、`migrate`、`sessions`、
`summaries`、`analytics`、`goals`、`categories`、`projects`、`resources`、`hourly`、`report`、`retention`、
`ai_conversations`、`insights_sql`、`search`、`like`、`tests`、`test_support`。
- 每个域一个文件，`impl super::Database { … }`；`conn`/`focused_threshold_ms` 私有，子模块天然可访问，**不需要放宽可见性**。
- 跨域私有 helper 只有一个：`like::like_match`（`pub(crate)`）。其余私有方法各属一个域。
- 测试夹具 `test_support.rs` 直接跑 `schema/hyprtrace.sql`（不再手写 DDL）+ `PRAGMA foreign_keys=ON`。


**日期列约定**：汇总表用本地 `date`；原始表用 `date_local`；只有 `started_at`/`occurred_at` 是 UTC 瞬时，本地小时在读取时用 `with_timezone(&chrono::Local)` 推导。`from > to` 是字典序比较，只对零填充 ISO 日期成立；非法日期静默返回空集。

**表归属**：schema 权威是仓库根的 `schema/hyprtrace.sql`，本 crate 通过 `src/schema.rs` 的 `include_str!` 嵌入（与 daemon 同一个文件，已无手工副本）。server 运行期**只写**：汇总表（仅重建）、`goals`、`app_categories`、`projects`/`project_rules`、`ai_conversations`，以及删除路径；**从不**插入 `sessions`/`activity_events`/`app_resources`/`disruptions`。`app_categories`/`projects`/`project_rules` 三张表刻意不放进共享 schema，由本 crate 的 `ensure_*` 自建自播种。

**性能与热点**：单连接单 `tokio::Mutex`，61 处 `db.lock().await` 串行化；重建/删除会整体重写汇总表并阻塞所有端点。`hourly_breakdown` 只要该日期在 `hourly_summary` 有 ≥1 行就直接信任它，不做交叉校验。`today_summary` 的空闲时长用 `LAG(...) OVER (ORDER BY started_at)` 扫**全部**已结束会话再按 `date_local` 过滤。

**其他约定**：LIMIT 钳制见上文；`set_categories` **不在事务里**（`set_projects`/`set_goals` 在）；LIKE 匹配是手写的 `like_match`（支持 `%`/`_`/`\` 转义、大小写不敏感），不是 SQL LIKE；`set_projects` 忽略客户端传来的 `sort_order`，按数组下标重排。

## 三、派生指标公式

- **专注时长**：查询时由 `insights::focus` 派生。**专注块**＝同一本地日期上连续的**同类**会话、累计 dwell ≥ 15 分钟；块内会话的 dwell 之和就是专注时长（用 dwell 而非墙钟跨度，run 中间的空白不会吹大数字）。`/api/summary` 的 `total_focused_ms`、`/api/efficiency` 的 `focus_ratio`、`/api/apps` 的每应用专注、`/api/timeline` 的每小时专注、`/api/status`（Waybar）的 `today_focused_ms`、`insights/fragmentation` 的块计数**全部**走它。**新增任何专注相关展示都必须走它**——`/api/status` 曾是最后一个漏网的（部署验证时才发现它比 Dashboard 少报）。数据库里的 `focused_ms` 列是**旧的**每会话阈值公式（`max(0, duration - 阈值)`，中位会话约 1 秒时恒为 0），仍被写入但不再是任何权威数字的来源。
- **效率分**（0..100）：`focus_score = clamp(40×focused/active, 0, 40)`（`focused` 即上面的派生专注时长）；`frag_score`：平均会话 900..5400s 得 30，更短按 `avg/900×30`，更长按 `30-((avg-5400)/10800)×10`（下限 0）；`late_score = clamp(15×(1-late_night_pct/100), 0,15)`；`disruption_score = clamp(15×(1-notifications/20), 0,15)`。**深夜窗口硬编码 23..5**（忽略 config 的 `late_night_*`）。**空数据的一天得 0 分**：公式只有一份（`insights::efficiency_subscore`，`efficiency_score` 直接调用它）——它曾经有两份逐字副本，只在空日上分道扬镳（30 vs 0）。
- **趋势预测 `/api/predict`**：只在**存在的日期**上做 OLS（不补零），结果钳制到 `[0, 16h]`；`predicted_today = max(拟合值, 今日实际)`。与 `/api/insights/forecast`（补零序列 + 周内因子 + 95% 区间）是**两套不同模型**。
- **洞察**：转移矩阵按行归一化到"该 app 的全部出边（含自环）"（`probability = count/out_total`，矩阵行和 ≈1，`edges` 不含自环）；预测的周内因子只用**有活动的日子**计算、无活动的星期回退 1.0、统一钳到 `0.2..=5.0`；碎片化的"重建成本"定义为 `Σ round(min(prev_dwell, 60s) × 0.5)`；共现的 lift/jaccard/support 都按"出现过的日期集合"算；异常的基线是中位数 + `1.4826×MAD`（MAD 塌缩时回退标准差），触发条件是 `|z| ≥ threshold` 或深夜占比 ≥0.35 或短会话占比 ≥0.75，severity 由 `|z| ≥ 1.5×threshold` 或"≥2 个触发条件"决定。所有除法都有零分母保护，输出保证有限。
- **项目统计**：按 class 取**优先级最高的** LIKE 规则归属项目；百分比分母包含未归类桶；未归类桶名 `未分类`、色 `#6b7280`。
- **工作区建议**：取每个 class 时长最长的工作区，置信度按占比 ≥70/≥40 分档；**完全并列时依赖 HashMap 迭代顺序，结果不确定**。

## 三·五、前端类型是生成的

`crates/hyprtrace-server/src/api_types.rs` 把 `models.rs` 渲染成 `web/src/lib/api-types.ts`，
`api_types_are_in_sync_with_the_models` 在不一致时失败。**改任何 serde 模型后**：

```bash
HYPRTRACE_WRITE_TYPES=1 cargo test -p hyprtrace-server api_types
```

两个约束：64 位整数必须渲染成 `number`（`Config::with_large_int("number")`——ts-rs 默认 `bigint`，JSON 给不出，前端还要对这些毫秒做算术）；
该模块是 `#[cfg(test)]`（渲染是构建期的事，放进二进制会多两个只被测试读的 `pub` 项 = dead-code 告警）。

## 四、鉴权（`auth.rs`）

两层，作用于整个 `/api` 嵌套路由（静态文件与 SPA fallback 在中间件之外）：

1. **Origin 白名单（始终启用）**：带 `Origin` 且主机不是回环（`localhost`/`*.localhost`/回环 IP 字面量，端口任意）→ `403`。没有 `Origin` 的请求（curl、脚本、waybar）直接放行。`127.evil.com`、`2130706433`、`192.168.x.x` 都被拒绝（有回归测试）。
2. **可选静态 token**：`server.auth_token` 非空才启用，`/health` 与 `/api/health` 豁免，其余要求 `X-Auth-Token: <token>` 或 `Authorization: Bearer <token>`（常量时间比较）。前端 `web/src/lib/auth.ts` 发 `X-Auth-Token`，收到 401 会清掉本地 token。

**没有 CORS 层，这是刻意的**：开发走 Vite 同源代理，生产由本服务同源提供 UI，跨源永远不合法，所以不靠浏览器侧的 CORS 头。

## 五、后台任务（都在启动时 detached spawn，无关闭通道）

- **proactive**：间隔 `max(configured, 15) 分钟`，**启动时取一次**（改配置不生效）。取 DB 快照后在锁**外**调 AI，回复与上次相同则跳过；否则 `notify-send`。**无法关闭**（配 0 也会被抬到 15 分钟）——跑临时实例时必须隔离 HOME。
- **retention**：首次 5 分钟后，之后每 6 小时；`retention_days = 0` 时跳过；否则删除严格早于 cutoff 的**已结束**会话并重建汇总。
- **weekly_report**：每 60s 检查，`try_lock` 拿不到 config 就跳过本轮；开启且到点（星期匹配且时间 ≥ 设定值）时生成 markdown 到 `~/.local/share/hyprtrace/reports/` 并发通知；用内存 `HashSet` 去重（重启会重发同一周）。

## 六、AI 集成（DeepSeek Harness 是唯一运行时）

server 里**没有**进程内 LLM provider、agent 循环或工具执行器。`ai/service.rs::AiService` 常驻一个
`dsh --profile sdk --patch <渲染后的 overlay>` 子进程；`ai/bridge.rs` 通过 Unix socket 接 harness 内的
插件 `integrations/dsh/hyprtrace-bridge.mjs`，补齐官方 SDK 不给的能力（token 流、reasoning、取消、清单）。

- **SDK 协议**：stdio NDJSON JSON-RPC 2.0 —— `initialize` / `session/prompt` / `session/resume`；
  通知 `session.event`、`session.status`。子进程 stdout 只读 JSON-RPC，日志走 stderr（`RUST_LOG=debug` 可见 `dsh:` 前缀）。
- **bridge 协议**（冻结在 `docs/dsh-integration.md` / `.dsh-diag/deep-fusion/CONTRACT.md`）：
  插件→服务端 `hello` / `stream`（`agent/assistant-stream` 原样 frame）/ `status` / `agent-error` / `session-created`；
  服务端→插件 `cancel` / `models` / `tools` / `ping`。socket 路径 `$XDG_DATA_HOME/hyprtrace/dsh/bridge.sock`（0600），
  经 env `HYPRTRACE_BRIDGE_SOCK` 传给 dsh；插件 500ms 退避重连。
- **事件映射**（`ai/service.rs::TurnState`）：bridge `text-delta`→`text`、`reasoning-delta`→`reasoning`；
  SDK `tool/call`/`tool/result`→`tool_call`/`tool_result`；`turn/end` completed/aborted/error→`done`/`cancelled`/`error`；
  bridge 未连时用 `assistant/message` 兜底（按 `(turn,step)` 去重，避免重复文本）。七种事件是前端硬契约。
- **会话**：`conversation_id` 直接当 DSH session id（缺省用 data dir 里持久化的默认 id）；子进程重启/模型切换后
  该对话首轮 `session/resume`。`DELETE /api/ai/conversations` 清历史并 `cancel_all`。
- **模型切换**：请求里的 `model` 与当前运行时不同 → 重启子进程并以该模型 `initialize`（会话 resume）；
  `GET /api/ai/models?refresh=1` 才启动 harness 取真实清单，不带 refresh 不启动（页面加载不付子进程启动成本）。
- **取消**：`POST /api/ai/cancel` 或客户端断连（`run_ai_agent` 监听 `ndjson_tx.closed()`）→ bridge
  `agent.cancel({kind:'user'},{keepInbox:true})`；流以 `cancelled` 终止（不再有 done）。
- **容错**：子进程 stdout EOF 用 `watch` 立即发现并中断本轮；`request_timeout_seconds` 超时丢弃子进程；
  每轮 `request/header` 的工具面守卫（必须含 `[ai.dsh].tools_prefix`，默认 `mcp__hyprtrace__`）防止 MCP 桥坏掉时模型裸答。
- **资产**：`integrations/dsh/overlay.template.yml`（6 占位符：`{{PROVIDER}} {{MODEL}} {{PROVIDER_OPTIONS}} {{API_KEY_ENV_LINE}} {{TOOLS_SCRIPT}} {{BRIDGE_SCRIPT}}`；
  渲染成 data dir 的 `overlay.yml`，0600；`{{PROVIDER_OPTIONS}}` 行模板自带 8 空格，Rust 只给续行加缩进）、
  `hyprtrace-mcp.mjs`（21 个只读 GET 工具，模型侧名 `mcp__hyprtrace__*`）、`hyprtrace-bridge.mjs`。
  `resolve_asset` 搜索：显式配置 → `$HYPRTRACE_DSH_DIR` → 用户 data dir → `/usr/share/hyprtrace/dsh` → 仓库检出 → 当前目录。
- **环境变量**：`HYPRTRACE_API_BASE`（按 `[server] host/port` 推导，0.0.0.0→127.0.0.1）、`HYPRTRACE_AUTH_TOKEN`、
  `HYPRTRACE_BRIDGE_SOCK`，由 server spawn dsh 时注入；overlay 用 `!!js "process.env.…"` 透传给 MCP/bridge 子进程。
- **周报/主动提醒**也走 `AiService::complete`；`ai_conversations` 只用于前端历史展示（model 列写 `dsh`）。
- **已删除的旧栈**：`ai/{ollama,openai,tools}.rs`、`AiManager`、`run_agent`、`/api/ai/chat/stream*`；
  `[ai.ollama]`/`[ai.openai]`/`[ai] default_provider` 等旧键被安全忽略。
- 细节/排障/E2E 配方：`docs/dsh-integration.md`；单测 `cargo test -p hyprtrace-server ai::`。

## 七、不变量（改之前先读）

1. **config ↔ AI 锁序**：`GET /api/config` 先克隆 config 再取 `ai.status()`；`PUT` 先落盘并 `drop(config)` 再 `ai.reconfigure()`。任何新 handler 都不要跨 await 持 config 锁。
2. 单连接单锁：不要在锁内做长任务；新增长查询要考虑它会阻塞所有端点。
3. `Database` 方法都是 `&self` + `unchecked_transaction()`，互斥完全依赖外层 Mutex。
4. **重建非事务**（先 DELETE 后 INSERT）→ 中断会留下空/半截汇总表，且 `hourly_breakdown` 会静默信任它。修的时候要和 daemon 的增量 upsert 语义对齐。
5. LIMIT 钳制是安全措施（负 LIMIT = 无限制）。`/api/sessions` 的 `page`/`per_page` 曾在 handler 与 SQL 两层都没有钳制，且 `offset` 是不可检查的 u32 乘法（debug 下 panic、release 下**静默回绕**到错误窗口）；现在统一由 `db::clamp_sessions_page`（1..500）处理，handler 与查询各调一次。新端点必须在两层都钳。
6. 日期语义：请求用本地日期，原始表用 `date_local`，汇总表用本地 `date`；时区变更会重切历史桶。
7. **200 + `{"error":...}`** 的约定：前端只检查非 2xx，改这里会连带改前端。
8. 前端契约：`web/src/lib/api.ts` 调用的端点清单、NDJSON **七种**事件名（text/reasoning/tool_call/tool_result/done/cancelled/error）、`conversation_id`、`X-Auth-Token` 头，都是硬契约。`PUT /api/goals` 是整体替换，而 AI 工具路径（MCP，只读）无此问题。
9. AI 工具 schema 在 `integrations/dsh/hyprtrace-mcp.mjs`（21 个只读 GET 代理），改参数名/描述即改模型可见契约；`models.rs` 的 serde 名仍是前端契约。
10. `Config::load()` 解析失败 = 启动失败（不是回退默认值）。`ensure_categories`/`ensure_projects` 的启动重试循环是防 systemd 崩溃循环的，新增启动写入要放进那个循环里。
11. DSH 相关硬契约：7 种 NDJSON 事件（`AiEvent::to_json` 有单测）；`TurnState` 里 `tool/result` 的 name 来自先前 `tool/call` 的 callId 配对（不要改成无状态）；overlay 模板的 6 个占位符名/位置与 `render_overlay` 匹配（`{{PROVIDER_OPTIONS}}` 行自带 8 空格缩进）；bridge 协议见 `docs/dsh-integration.md`，改字段要同步插件与 Rust。

## 八、测试

```bash
cargo test -p hyprtrace-server    # 104 passed（auth 4 / config 7 / db 19 / desktop 7 / insights 18 / ai::bridge 3 / ai::service 15 / static_cache 1）
```

未覆盖：`efficiency_score`、`predict`、`workspace_recommendations`、`current_status`、所有 `insight_*` 的 SQL 方法、七个洞察 handler、鉴权端到端、config PUT 的 HTTP 往返、`run_ai_agent` 的完整 HTTP 往返（DSH 子进程只能在真机跑，E2E 配方见 `docs/dsh-integration.md`）。新增 SQL/公式时，优先把**纯函数**放进 `insights.rs`（已有 18 个用例的模式），SQL 只做取数。
