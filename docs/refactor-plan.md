# HyprTrace 解耦与改进规划

> 状态：执行中。分支 `refactor/decouple`。每完成一步勾选，并在同一 commit 内更新本文件。
> 全部改动遵守 `AGENTS.md` 的不可协商约束：不碰运行中的 9420 实例、不对同一数据库起第二个 daemon、
> 不打印 `config.toml` 里的 API key、schema 改动两侧同步、前端只用语义 token。

## 已批准的决策（2026-09-10）

| 决策 | 选择 |
|---|---|
| 主线顺序 | **解耦先行**：先纯移动拆分，再做语义与新功能 |
| schema 单源化 | **共享 `schema/*.sql` + 两处 `include_str!`**（不新建 crate、不引依赖） |
| 允许的新依赖 | `rusqlite` 开启 `bundled`、`ts-rs`（Rust→TS 类型）、`@tanstack/react-query`（前端数据层） |
| focus 语义 | **允许改语义并重建历史汇总** |

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
- [ ] 1a `schema/hyprtrace.sql` 单源化：两份 DDL 合并为一个文件 + 两处 `include_str!`
- [ ] 1b `server/db.rs`(3412) → `server/db/*.rs`（按域拆 14 个模块 + `tests.rs`）
- [ ] 1c `server/insights.rs`(2075) → `server/insights/*.rs`（10 个域模块，测试随域下放）
- [ ] 1d `daemon/db.rs`(1469) → `daemon/db/*.rs`
- [ ] 1e `web/src/pages/Settings.tsx`(1525) → `pages/settings/*`，`Toggle/SaveButton/Spinner/AnimatedRow`
      上提到 `components/ui/`
- [ ] 1f `daemon/config.rs` 与 `server/config.rs` 的共享部分（158 行）单源化

### Phase 2 — 正确性欠债（各配回归测试，各自独立 commit）
- [ ] 2a `rebuild_*` 事务化
- [ ] 2b `resume_session` 尊重 `record_titles`
- [ ] 2c `disruptions` / `app_resources` 纳入保留策略
- [ ] 2d 收敛 `migrate_v6` 的重建分歧（三处实现统一语义 + 迁移测试）

### Phase 3 — 语义统一（需重建历史汇总）
- [ ] 3a `focused_ms` 改为连续专注块累计（复用 `insights` 现有块检测），daemon/server/脚本三处一致
- [ ] 3b daemon 侧会话合并/debounce（同 class 且间隔 < N 秒不新建会话），消掉 1.1 秒中位会话
- [ ] 3c 提供重建入口与一次性迁移，保证新旧数据语义一致

### Phase 4 — 新功能
- [ ] 4a 全局搜索（SQLite FTS5 + `bm25()`/`snippet()`，零新依赖）
- [ ] 4b `ts-rs` 由 Rust models 生成 `web/src/lib/types.ts`
- [ ] 4c `@tanstack/react-query` 接管前端数据层
- [ ] 4d 周期对比 / 复盘页
- [ ] 4e 应用级预算 + 预测式提醒（复用 `insights` 的 OLS）

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
- CRLF 文件：`daemon/config.rs`、`daemon/main.rs`、两个 `Cargo.toml`、`scripts/*.service`、`web/src/lib/api.ts`
  （朴素 diff/patch 会被行尾噪声干扰）
- 文档里的路径引用会过期：`AGENTS.md`、`.dsh/skills/hyprtrace-{architecture,server}/SKILL.md`
