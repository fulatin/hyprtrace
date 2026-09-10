# HyprTrace 解耦与改进规划

> 状态：执行中。分支 `refactor/decouple`。每完成一步勾选，并在同一 commit 内更新本文件。
> 全部改动遵守 `AGENTS.md` 的不可协商约束：不碰运行中的 9420 实例、不对同一数据库起第二个 daemon、
> 不打印 `config.toml` 里的 API key、schema 只在 `schema/hyprtrace.sql` 定义一次、前端只用语义 token。

## 已批准的决策（2026-09-10）

| 决策 | 选择 |
|---|---|
| 主线顺序 | **解耦先行**：先纯移动拆分，再做语义与新功能 |
| schema 单源化 | **共享 `schema/*.sql` + 两处 `include_str!`**（不新建 crate、不引依赖） |
| 允许的新依赖 | `rusqlite` 开启 `bundled`、`ts-rs`（Rust→TS 类型）、`@tanstack/react-query`（前端数据层） |
| focus 语义 | **允许改语义并重建历史汇总**；落地方式追加选定为 **A 方案（查询侧派生）**：服务端纯函数按需算，daemon 不改、零 schema 变更、不新建 crate |

## 基线（Phase 0 记录，用作每步回归的对照）

- `cargo test --workspace`：daemon **47 passed**、server **58 passed**（56 `#[test]` + 2 `#[tokio::test]`）
- `cd web && npx tsc --noEmit -p tsconfig.json`：0 error
- `git status`：clean，跟踪文件 112

每一步完成后必须仍然：daemon 47 / server 58、`tsc` 0 error、无新增 warning、SQL 与 serde 字段一字不改。

## 勘察结论摘要

**实测到的真问题**（只读查询运行中的实例）：

- 今日 618 个会话，**中位时长 1126ms**，平均 14.4s，**95.8% 会话 <1 分钟**，138 次切换
- `/api/efficiency` → `focus_ratio: 0.0`；Dashboard「Focus time <1m」
- 根因：`focused_ms = max(0, duration_ms - 1200s)` 在中位 1.1 秒的会话下**永远为 0**，
  且「减阈值」的公式使 25 分钟会话也只记 5 分钟
- 同时 `insights.rs` 的 `FOCUS_BLOCK_MS = 15min` 连续块定义给出的是合理数字
  ⇒ **同一个应用里有两个互相矛盾的「专注」定义**

**重复代码实测**：daemon 与 server 的 schema 副本当前**未漂移**（唯一差异是 server 额外建
`app_categories`/`projects`/`project_rules`，属 server 自有）；唯一真实分歧是 `migrate_v6` 的重建路径
（daemon 调 `rebuild_summaries_from_sessions`，server 调 `rebuild_daily/hourly_summary`，
在 `duration_ms = 0` 行、`date_local = ''` 行、`focused_ms` 来源三处不同）。

**已核实的缺陷**：

1. `idle_monitor.rs` 的 `resume_session` 无条件写真实 title，`record_titles = false` 隐私开关失效
2. `rebuild_daily_summary` / `rebuild_hourly_summary` 先 DELETE 后 INSERT 且**无事务**，中断后图表静默变空
3. `retention_days` 默认 0（关闭）；`disruptions` 无任何清理路径；`app_resources` 每 30s 一行

## 分期与检查点

### Phase 0 — 准备 ✅
- [x] 建分支 `refactor/decouple`
- [x] 记录基线（测试数、tsc、git 状态）
- [x] 本规划落盘

### Phase 1 — 解耦（纯移动，零行为变化）
- [x] 1a `schema/hyprtrace.sql` 单源化：两份 DDL 合并为一个文件 + 两处 `include_str!`
      （fresh-DB schema 逐字节比对通过；新增 3 个守卫测试/crate；快照 `schema/expected-columns.txt`）
- [x] 1b `server/db.rs`(3445) → `server/db/*.rs`（20 个域模块 + `tests.rs` + `test_support.rs`；
      字面量指纹 `94189d41…`（486 个）不变；67 行多重集差异里筛掉脚手架后只剩 7 行，全是测试辅助函数与 `like_match`
      因跨模块而加 `pub(crate)` 的签名行）
- [x] 1c `server/insights.rs`(2075) → `server/insights/*.rs`（11 个域模块，测试随域下放；字面量指纹 `73850b2f…` 不变）
- [x] 1d `daemon/db.rs`(1507) → `daemon/db/*.rs`（7 个文件；字面量指纹 `c8525c83…` 不变；行多重集差异只有 mod/use/impl 包装/一处签名放宽）
- [x] 1g 顺带修掉 1a 自己引入的一处回归：`EXPECTED_SCHEMA` 只在测试里用，非测试构建下是 dead-code warning
      （测试构建看不到，只有真 `cargo build` 才暴露）→ 改为 `#[cfg(test)]`，server 告警 6 → 5、daemon 回到基线
- [ ] 1e `web/src/pages/Settings.tsx`(1525) → 与 4c 的 react-query 合并成一次改动（**不能当纯移动做**：
      44 个 `useState` 与取数逻辑都在父组件里，切分必然是真正的重构，需按仓库习惯做浏览器视觉核对）
- [ ] 1f `daemon/config.rs` 与 `server/config.rs` 的共享部分（158 行）单源化
      —— 不新建 crate：沿用 schema 的做法（共享快照文件 + 两端断言），让漂移变成测试失败

### Phase 2 — 正确性欠债（各配回归测试，各自独立 commit）
- [ ] 2a `rebuild_*` 事务化
- [x] 2b `resume_session` 尊重 `record_titles`（`9ba0639`：抽出 `privacy::recorded_title`，两条写 title 的路径统一走它）
- [ ] 2c `disruptions` / `app_resources` 纳入保留策略
- [ ] 2d 收敛重建口径分歧。已知**四处**，不止最初看到的一处：
      ① `migrate_v6` 的重建路径两侧不同（daemon `rebuild_summaries_from_sessions` vs server `rebuild_daily/hourly_summary`），
      在 `duration_ms = 0` 行、`date_local = ''` 行、`focused_ms` 来源三处不同；
      ② `end_session` 增量写的 `focused_session_count` 用 `activity_state == "focused"`，
      而重建路径用 `focused_ms > 0` 计数——同一个字段两种口径；
      ③ server 的 `migrate_v6` 跑在 `migrate_v7` 之前，此时 `date_local` 可能还是空串，
      而 `rebuild_daily_summary` 正是按 `date_local <> ''` 过滤 → 会重建出一张**空表**；
      ④ `repair-negative-durations.sh` 是第三份实现。
- [x] 2e `finalize_orphaned_sessions` 关闭孤儿会话后不维护汇总表（`56b03ac`）：
      改为在同一事务内按 LOCAL 开始日期**作用域重建**（`write_summaries` 被抽出，`migrate_v6` 仍走全量路径），
      并补齐 `focused_ms`/`date_local`。回归测试已验证「撤掉修复即变红」。
- [ ] 2f **`sessions_paginated` 的 `(page-1) * per_page` 未做受检乘法**（拆分时发现）：
      `page` 巨大时 `usize` 溢出 → 转 `i64` 变负 → SQLite 把负 LIMIT 当**无限制**（架构技能不变量 5 的原型）。
      handler 侧也没有钳制。修法：两侧都 clamp，并加一个 `page = usize::MAX` 的回归测试。
- [ ] 2g **`db/test_support.rs` 手写 8 条 `CREATE TABLE`**（拆分时发现）：与 `schema/hyprtrace.sql` 单源规则冲突，
      夹具会静默漂移（其中 `sessions` 已被声明两次）。修法：夹具改为从共享 schema 建表。
- [ ] 2h **效率分在空日有两个定义**：`efficiency_score` 返回 30（late 15 + disruption 15），
      而 `insights::efficiency_subscore` 对同一天返回 0，`insight_disruption_impact` 用的是后者。
      与 Phase 3 的「全应用只有一个定义」是同一主题，建议并入 3 一起做。

### Phase 3 — 语义统一（**已定 A 方案：查询侧派生**）
用户已选定：专注时长改为**由服务端用纯函数按需派生**，daemon 一行不改、零 schema 变更、不新建 crate。
- [ ] 3a 把 `insights` 的专注块检测（同类会话连续成串、累计 ≥ 15 分钟）扩成「按日给出成员会话」的纯函数
- [ ] 3b Dashboard / 效率分 / 目标 / Insights 全部改用它——全应用只剩一个「专注」定义
- [ ] 3c `daily_summary.focused_ms` 降级为 legacy（保留列供兼容，文档与代码注释明确标注，不再作为权威来源）
- [ ] 3d 会话合并/debounce 暂缓：A 方案下既非必需，也能避免改动 daemon 写入路径

### Phase 4 — 新功能
- [ ] 4a 全局搜索（SQLite FTS5 + `bm25()`/`snippet()`，零新依赖）
- [ ] 4b `ts-rs` 由 Rust models 生成 `web/src/lib/types.ts`
- [ ] 4c `@tanstack/react-query` 接管前端数据层
- [ ] 4d 周期对比 / 复盘页
- [ ] 4e 应用级预算 + 预测式提醒（复用 `insights` 的 OLS）

## 已验证的证据（可复现，不是断言）

**1. 纯移动（每次拆分都跑）**

```bash
# 字面量多重集指纹：抓任何 SQL / 格式串改动
cat <新目录>/*.rs | grep -o '"[^"]*"' | sort | md5sum   # 必须等于拆分前单文件的指纹
# 行多重集：抓任何逻辑/数字/注释改动（差异只允许是 mod/use/impl 包装/pub(crate) 放宽）
git show <拆分前>:<旧文件> | sed 's/[[:space:]]*$//' | sort | uniq -c > /tmp/a
cat <新目录>/*.rs | sed 's/[[:space:]]*$//' | sort | uniq -c > /tmp/b ; diff /tmp/a /tmp/b
```

| 目标 | 指纹 | 结果 |
|---|---|---|
| `server/db.rs` → `db/*.rs` | `94189d41…`（486 个字面量） | 一致；67 行差异筛掉脚手架后只剩 7 行签名放宽 |
| `server/insights.rs` → `insights/*.rs` | `73850b2f…`（199） | 一致；105 行差异全是脚手架 |
| `daemon/db.rs` → `db/*.rs` | `c8525c83…`（214） | 一致；差异只有 mod/use/impl 包装 + 一处 `pub(super)` |
| `schema/hyprtrace.sql` | 全新库 `sqlite_master`+`pragma table_info` 转储 | 与改动前**逐字节相同**（含列顺序、可空性、默认值） |

**2. 新模块是否真的被编译**：`db` 20/20、`insights` 11/11 个文件都有 `mod` 声明；并各注入过一次语法错误确认构建会失败（防「文件写了但没声明 = 静默不编译」）。

**3. 回归测试是否真的有效**：2e 的测试在撤掉修复后报 `the finalised session must appear in daily_summary: QueryReturnedNoRows`，装回修复才通过。

**4. 真实数据端到端等价（本轮做的集成验证）**

按 `hyprtrace-ops` 的隔离流程：`sqlite3 <真实库> ".backup"` 取 WAL 一致性快照（21,519 会话 / 242 汇总日 / 8,935 资源行 / 14,621 打断），
手写一份**不含任何 api_key** 且指向副本的配置，用隔离 `HOME`+`XDG_CONFIG_HOME` 起 9421 实例跑重构后的 binary，
与线上 9420（旧代码）**逐字节对比历史日期响应**：

- 13 个端点（summary/apps/efficiency/sessions 分页/titles/timeline/resources/disruptions/classes/categories/projects/project stats/report）**完全一致**
- 4 个端点的差异**全部定位在「今天」这一仍在写入的尾部**：`activity/daily` 前 29 天完全一致，只有今天不同；
  `activity/events` 对齐 200 行后**身份字段（id/session_id/state/started_at）0 处不一致**，仅 1 行的 `ended_at`/`duration_ms` 不同——
  即快照时仍未闭合、之后被线上 daemon 闭合的那一条；`goals` 与 `workspace/recommendations` 的差异同源于今日总量

收尾确认：9421 端口释放、线上 9420 与 systemd 服务未受影响、工作区干净。

## git 约定

- 纯移动 commit 与行为变更 commit **严格分离**，提交信息沿用现有 conventional commits 风格
- 每步一个 commit，且该 commit 自身就能通过 CI 三步
- 拆分类 commit 的信息标注「no behavior change」，便于 `git show -M` 复核
- 不 push、不重启服务、不动 9420

## 已知陷阱（拆分时逐条核对）

- **新增文件忘了写 `mod` 声明 = 静默不编译**（无 error 无 warning），每步用 `cargo check` 的输出确认新模块出现
- `delete_goal` 上的 `#[allow(dead_code)]` 要跟着方法走
- `like_match` 是唯一跨组的私有 helper → 拆出后需 `pub(crate)`；其余私有方法都只属一个组
- `insights::is_late_night_hour` 带 `#[cfg(test)]` 门，不能改成模块级 `pub use`（会把测试专用符号泄漏进 release）
- 用显式 `pub use` 而非 glob re-export（避免未来同名符号静默遮蔽）
- 测试辅助函数（`test_db`/`insert_session`/`project_test_db`/`tracked_session`）构造 `Database` 字面量，
  只能在 `db` 的后代模块里编译 → 放 `db/test_support.rs`
- **告警只在真构建里出现**：`pub` 但只有测试读的常量，`cargo test` 不报警、`cargo build`/`cargo check -p` 才报（1g 就是这么漏掉的）。
  比告警集合时要用同一种构建方式，别拿 `cargo test` 的输出当基线。
- CRLF 文件：`daemon/config.rs`、`daemon/main.rs`、两个 `Cargo.toml`、`scripts/*.service`、`web/src/lib/api.ts`
  （朴素 diff/patch 会被行尾噪声干扰）
- 文档里的路径引用会过期：`AGENTS.md`、`.dsh/skills/hyprtrace-{architecture,server}/SKILL.md`
