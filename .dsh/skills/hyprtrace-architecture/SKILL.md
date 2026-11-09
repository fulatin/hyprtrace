---
name: hyprtrace-architecture
description: HyprTrace 全局架构地图：三个组件（daemon / server / web）的职责边界、数据流、目录与文件对照、时间与日期语义、schema 归属、跨组件不变量，以及"要改 X 应该动哪里"的路由表。
whenToUse: 在本仓库做任何改动之前；不确定某个功能属于 daemon、server 还是前端时；需要快速定位代码位置、理解数据从 Hyprland 到页面的完整链路时。
---

# HyprTrace 架构地图

Hyprland 窗口时间追踪器。三个独立进程/产物共享**一个 SQLite 文件**（WAL 模式），彼此不通过 RPC 通信：

```
Hyprland IPC ──▶ hyprtrace-daemon ──▶ SQLite ──▶ hyprtrace-server ──▶ Web SPA
                 (Rust, std::thread)   (WAL)      (Rust, Axum)         (React 18)
                                                    ▲                        │
                                                    └──── /api/* ────────────┘
```

| 组件 | 语言/栈 | 拥有什么 | 不做什么 |
|---|---|---|---|
| `hyprtrace-daemon` | Rust，纯 `std::thread`，无 async | 监听 Hyprland 事件写 `sessions`；空闲/输入/资源/打断/目标/作息监控；**schema 的权威定义** | 不做 HTTP，不读 `server.*` / `ai.*` 配置 |
| `hyprtrace-server` | Rust，Axum + tokio，**单连接 + 单 Mutex** | 全部 `/api/*`、静态文件服务、AI 集成、`app_categories`/`projects`/`goals` 等表的运行期写入 | 从不写 `sessions`/`activity_events`/`app_resources`/`disruptions`（删除除外） |
| `hyprtrace-common` | Rust 小库（无 runtime） | 两个二进制必须逐字一致的叶子件：XDG 路径 / `~` 展开、D-Bus 通知（zbus + notify-send 兜底） | 不放业务逻辑，不读配置、不碰数据库 |
| `hyprtrace-web` | React 18 + TS + Tailwind 3 + Recharts + framer-motion | 全部 UI；所有数据经 `/api` 获取 | 不直连数据库 |

## 目录与文件对照

```
crates/hyprtrace-common/src/            # 共享件（daemon + server 都依赖）
  paths.rs                   config_dir()（$XDG_CONFIG_HOME）+ expand_tilde()
  notify.rs                  send()/send_critical()：zbus org.freedesktop.Notifications，失败才回退 notify-send

crates/hyprtrace-daemon/src/           # 无 lib.rs，测试都在 #[cfg(test)] mod tests
  main.rs            (314)   启动顺序、线程孵化、退出码契约
  config.rs          (288)   ~/.config/hyprtrace/config.toml 的读写（daemon 首次运行会创建）
  schema.rs           (20)   include_str! 嵌入 schema/hyprtrace.sql
  privacy.rs          (44)   recorded_title()：写 sessions.title 的**唯一**允许/拒绝判定
  db/               (1500)   mod(结构体+open) / migrations / sessions / resources / disruptions / goals / tests
  listener.rs        (131)   Hyprland 事件 → 会话开关（唯一的窗口变更写入口）
  idle_monitor.rs    (407)   空闲判定（evdev > loginctl > Hyprland 事件）+ focused/idle 状态机
  input_monitor.rs   (134)   evdev 键鼠活动（可选，需要 input 组权限）
  resource_monitor.rs(216)   30s 采样当前会话进程的 CPU/内存 → app_resources
  disruption_monitor.rs(202) dbus-monitor 通知 + wl-paste 剪贴板 → disruptions
  goal_monitor.rs    (283)   目标 50%/100% 与连续聚焦休息提醒（发通知）
  wellbeing_monitor.rs (89)  深夜提醒，可调 hyprlock

crates/hyprtrace-server/src/
  main.rs                   启动、resolve_web_dir、SPA fallback、后台任务孵化
  schema.rs                 include_str! 嵌入同一份 schema + EXPECTED_SCHEMA（cfg(test) 快照）
  api_types.rs              由 Rust 模型渲染 web/src/lib/api-types.ts + 漂移测试（cfg(test)）
  db/                (3400)  mod(结构体+open) / migrate / sessions / summaries / analytics / goals /
                             categories / projects / resources / hourly / report / retention /
                             ai_conversations / insights_sql / search / like / tests / test_support
  models.rs                 全部 serde 模型（字段名即前端/AI 契约）——**再加字段要重新生成 api-types.ts**
  routes/{mod,data,config,ai,insights}.rs            路由表 / 数据 / 配置读写 / AI / 洞察
  insights/          (2000)  insights 公式（纯函数，无 DB）：focus / transitions / forecast / rhythm /
                             fragmentation / cooccurrence / disruption / anomalies / stats / time / session
  auth.rs                   Origin 白名单 + 可选静态 token
  desktop.rs                .desktop 解析 → 应用友好名/图标（OnceLock 全局缓存）
  proactive.rs / retention.rs / weekly_report.rs     三个后台任务
  ai/{mod,service,bridge}.rs  DeepSeek Harness 运行时：spawn `dsh --profile sdk`（stdio JSON-RPC）、
                             多会话/流式/取消/模型工具清单、Unix-socket 桥、overlay 渲染
integrations/dsh/           overlay 模板 + MCP 工具 server（21 只读）+ bridge 插件（流式/取消）

web/src/
  lib/api.ts         所有 REST 调用（含 insights 命名空间与 search）
  lib/api-types.ts   **生成物**：由 Rust 模型渲染，勿手改（见 api_types.rs）
  lib/types.ts       再导出生成类型 + 只属于客户端的请求体/包装类型
  lib/queries.ts     react-query 数据层：一个端点一个 key，共享 ['config'] + 保存后 invalidate
  lib/transport.ts   NDJSON agent 流 → AI SDK 消息流
  lib/theme.tsx      深色/浅色/跟随系统（localStorage: hyprtrace.theme）
  lib/motion.ts      动效预设（spring / fadeUp / pageTransition / …）
  components/ui/     设计系统基础件（Card/Panel/Reveal/Feedback/CountUp/Controls/AnimatedRow/…）
  components/insights/  洞察可视化（矩阵/流向图/预测/热力图/和弦图/散点/异常）
  pages/             Dashboard, Insights, Apps, Timeline, Sessions, Titles, Search, AIChat, Settings
  pages/settings/    Settings 的 9 个 section，各自持有草稿与保存处理器（父组件只剩 rail + scroll-spy）
  DESIGN.md          设计系统约定（颜色 token、组件类、动效规则）——改 UI 前必读

schema/hyprtrace.sql          **唯一**的 DDL 权威（两个 crate include_str! 同一份）
schema/expected-columns.txt   全新库 schema 快照，两个 crate 的 migrate_* 测试断言它

## 时间与日期语义（最容易踩的坑）

- `sessions.started_at` / `disruptions.occurred_at` 存 **UTC RFC3339 字符串**，用 `Utc::now().to_rfc3339()` 写入。字符串排序即时间排序，**前提是所有写入方用同一时区偏移**；写入本地偏移的字符串会静默破坏"当前会话"、资源归因与 `current_focused_duration_ms`。
- `date_local`（sessions/disruptions/activity_events/app_resources）是**本地日历日** `YYYY-MM-DD`，由 `chrono::Local` 从 UTC 时刻推导。原始表按日期过滤一律用 `date_local`。
- `daily_summary.date` / `hourly_summary.date` 同样是本地日期（列名没有 `_local` 后缀，别被误导）；`hourly_summary.hour` 是本地小时。
- 跨午夜的会话**整体归到开始那天/那个小时**。
- API 的 `from`/`to` 都是本地日期字符串，比较是字典序，只对零填充的 ISO 日期成立。
- 改机器时区会重新切分历史小时桶，并可能让 `hourly_summary` 与从 `sessions` 重算的结果不一致；`date_local` 只在为空时回填，之后永不修正。

## Schema 归属

- **权威定义只有一份：`schema/hyprtrace.sql`**（仓库根 `schema/`）。两个 crate 各有一个 `src/schema.rs`，用 `include_str!` 把同一个文件嵌进来（`SCHEMA_SQL`），在 `Database::migrate()` 开头 `execute_batch`。历史上两侧各有一份手抄副本（`KEEP IN SYNC`），已合并——不要再在任何 `db.rs` 里写 DDL。
- `src/schema.rs` 里还有 `EXPECTED_SCHEMA`（`schema/expected-columns.txt` 快照），两个 crate 的 `migrate_fresh_db_matches_the_canonical_schema` / `migrate_upgrades_a_legacy_database_to_the_canonical_schema` / `migrate_is_idempotent` 会断言它；改 DDL 后按测试提示重新生成快照。
- 没有 `user_version` 之类的版本闸门：`migrate()` **每次启动都跑全部迁移**，所以每个迁移必须幂等。
- 共享 DDL 描述的是**全新库的最终形状**，并且刻意与旧库升级后的形状保持一致（由 ALTER 追加的列在 DDL 里声明为 nullable-with-default）。旧库的列升级仍由各 crate 的 `migrate_v2/v3/v7`（`add_column_if_missing`）完成，不能删。
- 改列/加表后保持 `scripts/repair-negative-durations.sh` 的列预期（它读 `sessions`/`activity_events`/`daily_summary`/`hourly_summary`）。
- server 额外拥有 `app_categories`、`projects`、`project_rules`（`ensure_*` 在启动时创建并播种 47 条默认分类规则）——这三张表**刻意不在**共享 schema 文件里。

表清单（v1..v7）：`sessions`(+activity_state, focused_ms, pid, date_local)、`daily_summary`(+focused_ms, focused_session_count)、`hourly_summary`、`activity_events`、`app_resources`、`disruptions`、`goals`、`ai_conversations`(+complete)、`app_categories`、`projects`、`project_rules`。

## 跨组件不变量（改动前必须确认）

1. **专注时长不再是汇总表里的列，而是查询时派生的。** 唯一权威定义在 `server/src/insights/focus.rs`：一个**专注块**＝同一本地日期上连续的同类会话，累计 dwell ≥ 15 分钟；块内会话的 dwell 之和就是专注时长（用 dwell 而不是墙钟跨度，run 中间的空白不会把数字吹大）。
   数据库里那四个 `focused_ms` 列与 `focused_session_count` 记录的是**旧的**每会话阈值公式 `max(0, duration_ms - focused_threshold_seconds)`——中位会话约 1 秒时它**恒为 0**，所以整卡都是死的。这些列仍由 daemon 写入、由重建维护（避免读它们的东西崩），但**没有任何权威数字来自它们**。
   消费方（效率分的 40 分项、Dashboard 专注卡片、`app_ranking` 的每应用专注、`hourly_breakdown` 的每小时专注、`fragmentation` 的块计数）全部走 `insights::focus`。新增任何专注相关展示都必须走它，否则同一天又会出现两个答案。
2. **汇总表由 daemon 增量维护**（`end_session` 的事务里 upsert），server 只在重建/删除时整体重写。**两条路径的口径必须一致**，而这已经踩过雷：它们曾在 0 时长会话、`date_local` 未回填的行、`focused_ms` 来源、`focused_session_count` 计数四处各不相同（`08b769a` 统一）。
   现行规则：已完成会话全部计入（`duration_ms >= 0`）；分桶用 `started_at` 推导的 LOCAL 日期/小时（**不读** `date_local`）；`focused_ms` 按当前阈值重算；`focused_session_count = count(focused_ms > 0)`。
   `scripts/repair-negative-durations.sh` 是第三份实现，也必须同步（它会读配置里的阈值）。
3. **汇总重建已事务化**（`rebuild_daily_summary`/`rebuild_hourly_summary` 的 DELETE 与 INSERT 在同一个 `BEGIN IMMEDIATE` 里，`d2c26ca`）——曾经中断会留下空的/半截的汇总表，而 `hourly_breakdown` 只要看到该日期有 ≥1 行就直接信任它 → 图表静默变空。删会话路径（`delete_matching_sessions`）**仍未**整体事务化，是同类残留缺口。
4. **`class` 一律小写**写入（`listener.rs` 与 `idle_monitor.rs` 各一处），它是 `daily_summary`/`hourly_summary`/分类规则/目标 `target_key` 的聚合键；新增写入路径必须小写，否则聚合被拆开。
5. **限流钳制是安全措施**：`usize::MAX` 转 `i64` 会变成 `-1`，SQLite 把负 LIMIT 当"无限制"。新端点必须在 handler 和 DB 两层都钳制。
6. **HTTP 200 + `{"error": ...}`**：多数 handler 返回 `Result<Json<T>, Json<Value>>`，失败也是 200。前端 `fetchJSON` 只在非 2xx 抛错 → `{"error":...}` 会被当成正常数据，在组件里才炸。
7. **AGPL 无关但重要**：`sessions.title` 受 `daemon.record_titles` 控制（隐私开关），但 `idle_monitor::resume_session` 目前**无条件**写 title —— 已知的开关漏洞。
8. `app_resources` 无上限增长（每 30s 每会话一行），没有清理。

## "要改 X 就动哪里"

| 需求 | 位置 |
|---|---|
| 会话如何开始/结束、时长怎么算 | `daemon/db.rs::start_session` / `end_session`、`listener.rs` |
| 空闲判定、focused 状态 | `daemon/idle_monitor.rs`（`tick` + `resolve_idle_duration`） |
| 新增采集维度（新表） | daemon `db.rs::migrate*` **和** server `db.rs` 顶部副本 + 对应 monitor |
| 新增 REST 端点 | `server/routes/data.rs`（或 insights.rs）+ `routes/mod.rs` 注册 + `web/src/lib/api.ts` + `types.ts` |
| 新增派生指标/公式 | 纯函数放 `server/insights.rs`（便于单测），SQL 放 `server/db.rs` |
| 新的图表/页面 | `web/src/pages/` + `web/src/components/`，先读 `web/DESIGN.md` |
| 主题/配色/动效 | `web/src/index.css`（CSS 变量）、`tailwind.config.js`、`web/src/lib/motion.ts` |
| AI 能查什么 | `integrations/dsh/hyprtrace-mcp.mjs`（21 个只读 MCP 工具，名 `mcp__hyprtrace__*`）；人设/信任边界在 `integrations/dsh/overlay.template.yml` |
| AI 聊天由谁驱动 | `server/ai/service.rs::AiService`（DSH 子进程 + `routes/ai.rs::run_ai_agent`）；流式/取消/清单走 `server/ai/bridge.rs` + `integrations/dsh/hyprtrace-bridge.mjs` |
| AI 前端契约 | `server/ai/service.rs::AiEvent`（7 种 NDJSON 事件）+ `web/src/lib/transport.ts`；`conversation_id` 即 DSH session id |
| 安装/服务/打包 | `scripts/*.sh`、`scripts/*.service`、`.aur/`（gitignore，不提交） |

## 常用验证命令

```bash
cargo check --workspace && cargo test --workspace     # CI 的前两步
cd web && npx tsc --noEmit -p tsconfig.json           # 前端类型门（noUnusedLocals 会因未用导入报错）
curl -s localhost:9420/api/health                     # 运行中的实例是否活着
```

更细的按组件操作（构建、隔离实例、部署、打包、禁忌）见 `hyprtrace-ops` 技能；daemon 细节见 `hyprtrace-daemon`；API 与公式见 `hyprtrace-server`；前端见 `hyprtrace-web`。

## 验证与防漂移（这套仓库里已经建成的手法，新改动请沿用）

一次大规模解耦（拆 god file、单源化 schema、统一口径）总结出来的做法。**不是流程洁癖**：下面每一条都真的抓到过问题。

### 1. 仓库里已经存在的漂移守卫

| 守卫 | 保护什么 | 不符时怎么修 |
|---|---|---|
| `schema/expected-columns.txt` + 两个 crate 的 `migrate_*` 测试 | DDL 与「全新库的真实形状」一致（含列顺序、可空性、默认值） | 改 DDL 后重新生成快照（测试失败信息里有提示） |
| `web/src/lib/api-types.ts` + `api_types_are_in_sync_with_the_models` | 前端类型与 Rust 模型一致 | `HYPRTRACE_WRITE_TYPES=1 cargo test -p hyprtrace-server api_types` |
| `scripts/repair-negative-durations.sh` 的列预期 | 应急修复脚本不因加列而失效 | 加列时同步 |

三者都是「**快照 + 测试断言**」：把人工同步变成红灯，而不是靠记性。

### 2. 证明「纯移动」= 指纹 + 行多重集

拆文件时不要只说「我只搬了代码」，用两条机械证据：

```bash
# ① 字面量多重集指纹：抓任何 SQL / 格式串改动（拆分前后必须完全相同）
cat <新模块目录>/*.rs | grep -o '"[^"]*"' | sort | md5sum
# ② 行多重集：抓任何逻辑/数字/注释改动（差异只允许 mod/use/impl 包装/可见性放宽）
git show <拆分前>:<旧文件> | sed 's/[[:space:]]*$//' | sort | uniq -c > /tmp/a
cat <新模块目录>/*.rs      | sed 's/[[:space:]]*$//' | sort | uniq -c > /tmp/b
diff /tmp/a /tmp/b
```

再补一条**只在测试里用过**的检查：每个新文件都必须在 `mod.rs` 里有 `mod` 声明——**漏声明不会报错**，文件被静默忽略、方法凭空消失。故意在一个新文件里写个语法错误、确认构建失败、再改回来，是最快的自证方式。

### 3. 回归测试必须「撤掉修复就变红」

写完修复别急着提交：把修复临时还原，确认测试**确实失败**，并记下失败信息。本仓库已按此验证过 10+ 条修复（计划文档 `docs/refactor-plan.md` 有完整表格）。没做过这一步的测试等于没有测试。

### 4. 真实数据上的端到端等价（单元测试覆盖不到「迁移已有旧库」）

按 `hyprtrace-ops` 的隔离流程，用**数据库快照 + 隔离 HOME** 起一个临时实例，与运行中的旧版本**逐字段比对接口响应**。历史日期上必须逐字节相同；只允许「今天」这条仍在写入的尾部有差异，且每处差异都要能归因（哪几行是新增的、哪一行是刚被闭合的）。

### 5. 类型检查通过 ≠ 没坏：前端必须真的看一眼

真实案例：搜索页用 FTS5 的 `snippet()` 取摘要，类型与单测全绿，但在浏览器里读到的标题是 `zsh in hyprtra …`——**截在词中间**（trigram 分词器下 `snippet()` 数的是 3 字符序列而不是词）。只有把页面打开才发现。

### 6. 只被测试使用的 `pub` 项 = 真构建里的 dead-code 告警

`cargo test` **看不到**这类告警（测试构建里它们被用到了）。这个坑在本仓库被踩到 4 次（`EXPECTED_SCHEMA`、`MAX_SESSIONS_PER_PAGE`、`api_types::render`、`blocks()`）。规则：

- 只被测试读的常量/函数 → `#[cfg(test)]`；整个模块只服务测试就 `#[cfg(test)] mod x;`
- 比告警集合要用**同一种构建方式**，并且做**集合比对**（`cargo check --workspace 2>&1 | grep "^warning" | sort > baseline`），不要只数行数
- 发布用的 `pub use` 不要重导出测试专用符号（那是 unused-import 告警）

### 7. 沙箱与依赖（细节见 `hyprtrace-ops`）

本环境的文件沙箱只允许写工作区内：`npm` 必须 `--cache` 指向工作区，**新增 Rust 依赖需要一次提权**（cargo 要解包到 `~/.cargo`）。
