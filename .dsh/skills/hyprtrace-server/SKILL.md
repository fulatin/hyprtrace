---
name: hyprtrace-server
description: hyprtrace-server 参考：全部 /api 端点与其参数钳制、SQL 层与日期列约定、每个派生指标的确切公式（效率分、趋势预测、洞察七件套）、鉴权与 Origin 白名单、三个后台任务、AI provider 与 25 个 agent 工具、以及会踩坑的不变量。
whenToUse: 新增或修改 REST 端点、改派生指标/统计算法、排查接口数据不对、处理鉴权/CORS、改 AI 集成或 agent 工具、改配置读写时。
---

# hyprtrace-server 参考

Axum 0.7 + tokio，binary-only（无 `lib.rs`，测试全是内联单测）。`Arc<AppState>{ db: tokio::Mutex<Database>, config: tokio::Mutex<Config>, ai: tokio::Mutex<AiManager> }`，`Database` 内部是**一个 rusqlite 连接**（无连接池，也不套 `spawn_blocking`）。

## 一、HTTP 表面

**约定**：多数 handler 是 `Result<Json<T>, Json<Value>>`，失败也返回 **HTTP 200 + `{"error": ...}`**。只有 `PUT /api/projects`（400/500）、`/api/ai/chat/stream/text` 与 `/api/ai/chat/agent`（400）会返回真实状态码。未匹配的 `/api/*` → 404 JSON；其余路径落到 SPA fallback（200 + index.html，故意用 `.fallback()` 而不是 `.not_found_service()`，否则前端路由刷新会 404）。

数据端点（`routes/data.rs`）：

| 端点 | 关键参数（默认 → 钳制） | 返回 |
|---|---|---|
| `GET /api/health` | — | `{status,version}`（version 硬编码） |
| `GET /api/summary` | `date`(本地今天) | `TodaySummary` |
| `GET /api/apps` | from/to, `limit`(10 → 1..500，两层都钳) | `AppRank[]` |
| `GET /api/timeline` | `date` | `HourlyBucket[24]` |
| `GET /api/sessions` | from/to, `page`(1), `per_page`(50) **未钳制**, `class` | `{data,total,page,per_page}` |
| `DELETE /api/sessions` | from/to, `class` | `{deleted,rebuilt_summaries}` |
| `GET /api/app/:class/trend` | from/to, `granularity=hour` | `DailyTrend[]` |
| `GET /api/apps/classes` / `GET /api/apps/metadata` | from/to；`classes=a,b` | `string[]` / `{entries}`（不查库，走 desktop 缓存） |
| `GET /api/activity/events` / `daily` | `limit`(100 → 1..1000) / `days`(371 → **7..730**) | 事件列表 / 零填充的日均序列 |
| `GET /api/titles` | from(今天-6)/to, `class`, `limit`(50 → 1..500) | `TitleStat[]` |
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

配置：`GET /api/config`（**永不返回 API key**）；`PUT /api/config` 走 toml_edit 读改写（保留未知键与注释），`openai_api_key:""` 表示删除该键，改完重建 `AiManager`。**`auth_token` 不可通过 API 修改**，只能编辑 config.toml；`weekly_report_day` 无校验，写 >7 会静默关闭周报。

AI：`/api/ai/models`、`/tools`、`/chat`（普通）、`/chat/stream`（SSE，**无 `[DONE]`、无 error 事件**）、`/chat/stream/text`（纯文本）、`/chat/agent`（**NDJSON**，事件恰好五种：`text` / `tool_call` / `tool_result` / `done` / `error`，前端 `web/src/lib/transport.ts` 按此解析）、`/report/weekly`、`/conversations`（GET/DELETE）。

## 二、SQL 层（`db.rs`）

**日期列约定**：汇总表用本地 `date`；原始表用 `date_local`；只有 `started_at`/`occurred_at` 是 UTC 瞬时，本地小时在读取时用 `with_timezone(&chrono::Local)` 推导。`from > to` 是字典序比较，只对零填充 ISO 日期成立；非法日期静默返回空集。

**表归属**：schema 权威在 daemon，本 crate 顶部是手工副本（`KEEP IN SYNC`）。server 运行期**只写**：汇总表（仅重建）、`goals`、`app_categories`、`projects`/`project_rules`、`ai_conversations`，以及删除路径；**从不**插入 `sessions`/`activity_events`/`app_resources`/`disruptions`。

**性能与热点**：单连接单 `tokio::Mutex`，61 处 `db.lock().await` 串行化；重建/删除会整体重写汇总表并阻塞所有端点。`hourly_breakdown` 只要该日期在 `hourly_summary` 有 ≥1 行就直接信任它，不做交叉校验。`today_summary` 的空闲时长用 `LAG(...) OVER (ORDER BY started_at)` 扫**全部**已结束会话再按 `date_local` 过滤。

**其他约定**：LIMIT 钳制见上文；`set_categories` **不在事务里**（`set_projects`/`set_goals` 在）；LIKE 匹配是手写的 `like_match`（支持 `%`/`_`/`\` 转义、大小写不敏感），不是 SQL LIKE；`set_projects` 忽略客户端传来的 `sort_order`，按数组下标重排。

## 三、派生指标公式

- **效率分**（0..100）：`focus_score = clamp(40×focused/active, 0, 40)`；`frag_score`：平均会话 900..5400s 得 30，更短按 `avg/900×30`，更长按 `30-((avg-5400)/10800)×10`（下限 0）；`late_score = clamp(15×(1-late_night_pct/100), 0,15)`；`disruption_score = clamp(15×(1-notifications/20), 0,15)`。**深夜窗口硬编码 23..5**（忽略 config 的 `late_night_*`）。**空数据的一天得 30 分**而不是 0。注意 `insights::efficiency_subscore` 是另一套（把 focus_ratio 钳到 0..1，且 active≤0 时返回 0），两者对空日不一致；洞察的 disruption-impact 用的是后者。
- **趋势预测 `/api/predict`**：只在**存在的日期**上做 OLS（不补零），结果钳制到 `[0, 16h]`；`predicted_today = max(拟合值, 今日实际)`。与 `/api/insights/forecast`（补零序列 + 周内因子 + 95% 区间）是**两套不同模型**。
- **洞察**：转移矩阵按行归一化到"该 app 的全部出边（含自环）"（`probability = count/out_total`，矩阵行和 ≈1，`edges` 不含自环）；预测的周内因子只用**有活动的日子**计算、无活动的星期回退 1.0、统一钳到 `0.2..=5.0`；碎片化的"重建成本"定义为 `Σ round(min(prev_dwell, 60s) × 0.5)`；共现的 lift/jaccard/support 都按"出现过的日期集合"算；异常的基线是中位数 + `1.4826×MAD`（MAD 塌缩时回退标准差），触发条件是 `|z| ≥ threshold` 或深夜占比 ≥0.35 或短会话占比 ≥0.75，severity 由 `|z| ≥ 1.5×threshold` 或"≥2 个触发条件"决定。所有除法都有零分母保护，输出保证有限。
- **项目统计**：按 class 取**优先级最高的** LIKE 规则归属项目；百分比分母包含未归类桶；未归类桶名 `未分类`、色 `#6b7280`。
- **工作区建议**：取每个 class 时长最长的工作区，置信度按占比 ≥70/≥40 分档；**完全并列时依赖 HashMap 迭代顺序，结果不确定**。

## 四、鉴权（`auth.rs`）

两层，作用于整个 `/api` 嵌套路由（静态文件与 SPA fallback 在中间件之外）：

1. **Origin 白名单（始终启用）**：带 `Origin` 且主机不是回环（`localhost`/`*.localhost`/回环 IP 字面量，端口任意）→ `403`。没有 `Origin` 的请求（curl、脚本、waybar）直接放行。`127.evil.com`、`2130706433`、`192.168.x.x` 都被拒绝（有回归测试）。
2. **可选静态 token**：`server.auth_token` 非空才启用，`/health` 与 `/api/health` 豁免，其余要求 `X-Auth-Token: <token>` 或 `Authorization: Bearer <token>`（常量时间比较）。前端 `web/src/lib/auth.ts` 发 `X-Auth-Token`，收到 401 会清掉本地 token。

**没有 CORS 层，这是刻意的**：开发走 Vite 同源代理，生产由本服务同源提供 UI，跨源永远不合法，所以不靠浏览器侧的 CORS 头。

## 五、后台任务（都在启动时 detached spawn，无关闭通道）

- **proactive**：间隔 `max(configured, 15) 分钟`，**启动时取一次**（改配置不生效）。取 DB 快照后在锁**外**调 AI，回复与上次相同则跳过；否则 `notify-send`。**无法关闭**（配 0 也会被抬到 15 分钟）——跑临时实例时必须隔离 HOME。
- **retention**：首次 5 分钟后，之后每 6 小时；`retention_days = 0` 时跳过；否则删除严格早于 cutoff 的**已结束**会话并重建汇总。
- **weekly_report**：每 60s 检查，`try_lock` 拿不到 config 就跳过本轮；开启且到点（星期匹配且时间 ≥ 设定值）时生成 markdown 到 `~/.local/share/hyprtrace/reports/` 并发通知；用内存 `HashSet` 去重（重启会重发同一周）。

## 六、AI 集成

- `AiManager` 固定注册 `ollama` 与 `openai` 两个 provider（key 可回退 `OPENAI_API_KEY` 环境变量）。**调用 provider 前先 `snapshot()` 取副本，不要在持锁状态下发请求**。
- 流式差异：ollama 是 NDJSON，`tool_calls` **一次给全**；openai 是 SSE，`tool_calls` **跨 delta 分片**，在流结束后才 flush。两者 HTTP client 的总超时都是 300s。
- agent 循环：`MAX_ROUNDS = 6`、`MAX_TOOL_CALLS = 20`，首轮失败会以 `tools_enabled=false` 重试一次，超预算/截断都会发 `{"type":"error"}`。
- 25 个工具定义在 `ai/tools.rs`：DB 类（查汇总/排行/会话/目标/分类，以及 `set_goal`/`delete_goal`/`send_reminder`）+ 只读 Hyprland 类。**未知工具名会落到 Hyprland 分支**并返回 `Unknown tool`。system prompt 里编码了信任边界（窗口标题、通知文本、DB 值都当作不可信数据）与"未经明确要求不得调用 `set_goal(replace_all=true)`/`delete_goal`/`send_reminder`"的约束——不要削弱。
- 会话持久化：`ai_conversations.complete` 标记流式行是否完成；历史重建跳过"未完成且内容为空"的行，但**未完成但非空**的行会原样回灌进下一次 prompt。

## 七、不变量（改之前先读）

1. **config ↔ ai 锁序死锁**：`GET /api/config` 先 config 后 ai，`PUT` 释放 config 后再取 ai 却仍持有 ai …… 并发 GET+PUT 有死锁风险。新 handler 要么遵守 config→ai 顺序，要么绝不跨 await 持锁。
2. 单连接单锁：不要在锁内做长任务；新增长查询要考虑它会阻塞所有端点。
3. `Database` 方法都是 `&self` + `unchecked_transaction()`，互斥完全依赖外层 Mutex。
4. **重建非事务**（先 DELETE 后 INSERT）→ 中断会留下空/半截汇总表，且 `hourly_breakdown` 会静默信任它。修的时候要和 daemon 的增量 upsert 语义对齐。
5. LIMIT 钳制是安全措施（负 LIMIT = 无限制）。`/api/sessions` 的 `page`/`per_page` **没有**钳制，`offset` 是不可检查的 u32 乘法。
6. 日期语义：请求用本地日期，原始表用 `date_local`，汇总表用本地 `date`；时区变更会重切历史桶。
7. **200 + `{"error":...}`** 的约定：前端只检查非 2xx，改这里会连带改前端。
8. 前端契约：`web/src/lib/api.ts` 调用的端点清单、NDJSON 五种事件名、`X-Auth-Token` 头，都是硬契约。`PUT /api/goals` 是整体替换，而 AI 工具路径必须用合并语义——**不要统一**。
9. AI 工具依赖的字段名（手写 JSON 的 `get_today_summary`/`get_goals`，以及 `models.rs` 里所有 serde 名）改动即改模型可见契约。
10. `Config::load()` 解析失败 = 启动失败（不是回退默认值）。`ensure_categories`/`ensure_projects` 的启动重试循环是防 systemd 崩溃循环的，新增启动写入要放进那个循环里。

## 八、测试

```bash
cargo test -p hyprtrace-server    # 58 passed（auth 4 / config 7 / db 19 / desktop 7 / insights 18 / ai 2）
```

未覆盖：`efficiency_score`、`predict`、`workspace_recommendations`、`current_status`、所有 `insight_*` 的 SQL 方法、七个洞察 handler、鉴权端到端、config PUT 的 HTTP 往返、AI 流式解析器、`run_agent` 循环。新增 SQL/公式时，优先把**纯函数**放进 `insights.rs`（已有 18 个用例的模式），SQL 只做取数。
