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
| `hyprtrace-web` | React 18 + TS + Tailwind 3 + Recharts + framer-motion | 全部 UI；所有数据经 `/api` 获取 | 不直连数据库 |

## 目录与文件对照

```
crates/hyprtrace-daemon/src/           # 无 lib.rs，测试都在 #[cfg(test)] mod tests
  main.rs            (314)   启动顺序、线程孵化、退出码契约
  config.rs          (288)   ~/.config/hyprtrace/config.toml 的读写（daemon 首次运行会创建）
  db.rs             (1469)   schema 权威定义 + 会话开始/结束 + 汇总表增量写
  listener.rs        (131)   Hyprland 事件 → 会话开关（唯一的窗口变更写入口）
  idle_monitor.rs    (407)   空闲判定（evdev > loginctl > Hyprland 事件）+ focused/idle 状态机
  input_monitor.rs   (134)   evdev 键鼠活动（可选，需要 input 组权限）
  resource_monitor.rs(216)   30s 采样当前会话进程的 CPU/内存 → app_resources
  disruption_monitor.rs(202) dbus-monitor 通知 + wl-paste 剪贴板 → disruptions
  goal_monitor.rs    (283)   目标 50%/100% 与连续聚焦休息提醒（发通知）
  wellbeing_monitor.rs (89)  深夜提醒，可调 hyprlock
  notify.rs           (53)   zbus → notify-send 回退

crates/hyprtrace-server/src/
  main.rs            (120)   启动、resolve_web_dir、SPA fallback、后台任务孵化
  db.rs             (3400+)  schema 的**手工副本**（"KEEP IN SYNC"）+ 全部查询方法
  models.rs          (225)   全部 serde 模型（字段名即前端/AI 契约）
  routes/mod.rs       (98)   路由表 + AppState
  routes/data.rs     (761)   数据查询端点
  routes/config.rs   (179)   GET/PUT /api/config（toml_edit 读改写）
  routes/ai.rs       (744)   AI 对话（含 NDJSON agent 循环）
  routes/insights.rs (203)   7 个洞察端点（薄 handler，只做参数钳制）
  insights.rs       (2000+)  洞察公式 + 18 个单测（纯函数，无 DB）
  auth.rs            (194)   Origin 白名单 + 可选静态 token
  desktop.rs         (482)   .desktop 解析 → 应用友好名/图标（OnceLock 全局缓存）
  proactive.rs / retention.rs / weekly_report.rs   三个后台任务
  ai/{mod,ollama,openai,tools}.rs                   provider 抽象 / 流式解析 / 25 个 agent 工具

web/src/
  lib/api.ts         所有 REST 调用（含 insights 命名空间）
  lib/types.ts       与 models.rs 一一对应的 TS 类型
  lib/transport.ts   NDJSON agent 流 → AI SDK 消息流
  lib/theme.tsx      深色/浅色/跟随系统（localStorage: hyprtrace.theme）
  lib/motion.ts      动效预设（spring / fadeUp / pageTransition / …）
  components/ui/     设计系统基础件（Card/Panel/Reveal/Feedback/CountUp/…）
  components/insights/  洞察可视化（矩阵/流向图/预测/热力图/和弦图/散点/异常）
  pages/             Dashboard, Insights, Apps, Timeline, Sessions, Titles, AIChat, Settings
  DESIGN.md          设计系统约定（颜色 token、组件类、动效规则）——改 UI 前必读
```

## 时间与日期语义（最容易踩的坑）

- `sessions.started_at` / `disruptions.occurred_at` 存 **UTC RFC3339 字符串**，用 `Utc::now().to_rfc3339()` 写入。字符串排序即时间排序，**前提是所有写入方用同一时区偏移**；写入本地偏移的字符串会静默破坏"当前会话"、资源归因与 `current_focused_duration_ms`。
- `date_local`（sessions/disruptions/activity_events/app_resources）是**本地日历日** `YYYY-MM-DD`，由 `chrono::Local` 从 UTC 时刻推导。原始表按日期过滤一律用 `date_local`。
- `daily_summary.date` / `hourly_summary.date` 同样是本地日期（列名没有 `_local` 后缀，别被误导）；`hourly_summary.hour` 是本地小时。
- 跨午夜的会话**整体归到开始那天/那个小时**。
- API 的 `from`/`to` 都是本地日期字符串，比较是字典序，只对零填充的 ISO 日期成立。
- 改机器时区会重新切分历史小时桶，并可能让 `hourly_summary` 与从 `sessions` 重算的结果不一致；`date_local` 只在为空时回填，之后永不修正。

## Schema 归属

- **权威定义在 daemon**（`crates/hyprtrace-daemon/src/db.rs` 的 `migrate*`），server 在 `crates/hyprtrace-server/src/db.rs` 顶部保留一份**手工副本**（注释 `KEEP IN SYNC`）。
- 没有 `user_version` 之类的版本闸门：`migrate()` **每次启动都跑全部迁移**，所以每个迁移必须幂等。
- 加列/加表必须**同时改两处**，并保持 `scripts/repair-negative-durations.sh` 的列预期（它读 `sessions`/`activity_events`/`daily_summary`/`hourly_summary`）。
- server 额外拥有 `app_categories`、`projects`、`project_rules`（`ensure_*` 在启动时创建并播种 47 条默认分类规则）。

表清单（v1..v7）：`sessions`(+activity_state, focused_ms, pid, date_local)、`daily_summary`(+focused_ms, focused_session_count)、`hourly_summary`、`activity_events`、`app_resources`、`disruptions`、`goals`、`ai_conversations`(+complete)、`app_categories`、`projects`、`project_rules`。

## 跨组件不变量（改动前必须确认）

1. **`focused_ms` 是阈值启发式**，不是实测：`max(0, duration_ms - focused_threshold_ms)`（默认阈值 1200s）。改 `daemon.focused_threshold_seconds` 只影响**以后的**重建，不会追溯修正已写入的汇总。
2. **汇总表由 daemon 增量维护**（`end_session` 的事务里 upsert），server 只在重建/删除时整体重写。任何一方改了汇总语义，另一方必须跟上。
3. **server 的重建不是事务性的**（`rebuild_daily_summary`/`rebuild_hourly_summary` 先 DELETE 后 INSERT），中断会留下空的/半截的汇总表，而 `hourly_breakdown` 只要看到该日期有 ≥1 行就直接信任 `hourly_summary` → 图表会静默变空。这是已知缺陷。
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
| AI 能查什么 | `server/ai/tools.rs`（工具定义 + 派发）与 `server/ai/mod.rs`（system prompt、信任边界） |
| 安装/服务/打包 | `scripts/*.sh`、`scripts/*.service`、`.aur/`（gitignore，不提交） |

## 常用验证命令

```bash
cargo check --workspace && cargo test --workspace     # CI 的前两步
cd web && npx tsc --noEmit -p tsconfig.json           # 前端类型门（noUnusedLocals 会因未用导入报错）
curl -s localhost:9420/api/health                     # 运行中的实例是否活着
```

更细的按组件操作（构建、隔离实例、部署、打包、禁忌）见 `hyprtrace-ops` 技能；daemon 细节见 `hyprtrace-daemon`；API 与公式见 `hyprtrace-server`；前端见 `hyprtrace-web`。
