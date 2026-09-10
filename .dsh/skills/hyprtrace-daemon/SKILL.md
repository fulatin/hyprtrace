---
name: hyprtrace-daemon
description: hyprtrace-daemon 内部参考：启动顺序与线程模型、Hyprland 事件到会话生命周期的完整路径、end_session 事务与 duration/focused_ms 语义、六个监控线程的判定逻辑与降级行为、schema 迁移函数，以及会破坏数据的不变量清单。
whenToUse: 修改守护进程、排查"时长/专注时间/空闲状态不对"、会话丢失或重复、时间跳变、通知/提醒行为、evdev 或 D-Bus 相关问题时；新增采集维度时。
---

# hyprtrace-daemon 参考

纯 `std::thread`（无 async），`zbus` 用的是 blocking API。crate 是 binary-only（无 `lib.rs`），因此**所有测试必须是 `#[cfg(test)] mod tests` 内联单测**，没有 `tests/` 目录。

## 启动顺序（顺序是契约，不要重排）

`main.rs` 依次：env_logger → `Config::load()`（**缺失即创建**默认 config.toml）→ `ensure_db_dir` → `Database::open`（WAL / busy_timeout=5000 / foreign_keys=ON）→ `migrate()`（带 5 次 SQLITE_BUSY 退避重试：2/4/6/8/10s）→ `Arc<Mutex<Database>>`（**全进程单连接单锁**）→ `ActivityState` → idle / input / resource / goal / wellbeing 线程 → `DisruptionMonitor` → **`ctrlc::set_handler`（必须在 listener 线程之前）** → listener 线程（`catch_unwind`）→ 主线程阻塞在 `rx.recv()`。

| 线程 | 触发/周期 | 读的配置 |
|---|---|---|
| listener | 事件驱动（阻塞式 `start_listener()`） | `record_titles` |
| idle | `clamp(idle_timeout/4, 5, 30)` 秒 | `idle_timeout_seconds`, `focused_threshold_seconds` |
| input（evdev） | 100ms 轮询已打开的 fd | 仅 `enable_input_monitor` 决定是否启动 |
| resource | 30s（硬编码） | — |
| goal | 300s（硬编码） | `break_after_minutes` |
| wellbeing | 300s（硬编码） | `late_night_start_hour/end_hour`, `hyprlock_command` |
| disruption | dbus-monitor 行读取 + 5s 剪贴板轮询 | — |

- `let _disruption_monitor = ...` 是**有意的命名绑定**：`Drop` 会置 `running=false`，`let _ =` 会立刻停掉两个线程。
- 退出码：只有收到终止信号才 0，其余（包括 listener 正常返回）都是 1，以便 systemd `Restart=on-failure` 重启。
- 信号处理里用 20×50ms `try_lock` 尝试 `end_session`；拿不到锁就只记日志（可能留下孤儿会话，下次启动由 `finalize_orphaned_sessions` 收尾）。

## 会话生命周期

**开**（`listener.rs`，唯一的窗口变更写入点）：`mark_activity()` → **阻塞 `db.lock()`（锁中毒则丢弃该事件）** → `class = win_data.class.to_lowercase()` → `title`（`record_titles=false` 时写 `""`）→ **重新查 `Client::get_active()` 并核对 class**（防止 alt-tab 竞争把下一个窗口的 workspace/pid 记错；不匹配则 `workspace=""`, `pid=NULL`）→ `end_current_session()` → `start_session()`。

**关**（5 条路径，全部经 `end_session`）：下一次窗口变更 / 无活动窗口 / 空闲超时 / 信号 / 崩溃后的孤儿收尾。

`end_session` 的顺序不可调换：读 `started_at` → 时长 → 时钟跳变检测与重写 → `focused_ms` → 本地日期与小时 → **一个 `unchecked_transaction`** 内完成：`sessions` UPDATE、读 class/state、`daily_summary` upsert、`hourly_summary` upsert、`close_activity_event` → commit → 最后才移除 monotonic 标记（"end 已持久化后才丢弃起始点"）。

**时长**：正常结束用内存里的 `monotonic_starts[id].elapsed()`（`Instant`，免疫时钟回拨，钳制 ≥0）；无标记（上个进程留下的会话）回退到 `Utc::now() - started_at`；孤儿收尾用 `activity_events` 的最大 `ended_at` 并钳制到 `[started, now]`。

**时钟跳变四道防线**：(1) monotonic 时长；(2) `|wall_delta - duration| > 1000ms` 时把 `started_at` **重写**为 `now - duration` 并重算 `date_local`（注意：可能把会话挪到另一天/另一个小时桶）；(3) 启动时 `migrate_v6` 钳制负值并按需重建汇总；(4) 孤儿收尾钳制。

**`focused_ms` 是启发式**：`max(0, duration_ms - focused_threshold_ms)`，与会话实际是否"专注"无关；`activity_state` 才反映观测状态（`active`→`focused`→`idle`，以及事后补记的 `away`）。

## 六个监控

- **idle**：空闲来源优先级 = evdev `last_input`（若输入监控已激活）→ `loginctl show-user IdleHint/IdleSinceHintMonotonic`（**只有 `IdleHint=yes` 且有可解析的非零单调时间戳才采信**，否则视为"未知"继续回退）→ Hyprland 事件时间 → 无（不判空闲）。`away` 阈值 = `2 × idle_timeout`，且依赖**内存中**的 `last_idle_ended`（重启后丢失）。"恢复会话"分支实际上很难走到——任何活动标记都会清 `is_idle`，实践中下一个会话由 listener 开。
- **input（evdev）**：启动时枚举一次设备（要求 `EventType::KEY` 且至少一个非电源键），之后不再重扫（不支持热插拔）；`fetch_events` 的错误被忽略 → 设备失效后 `last_input` 冻结，会导致"用户在工作却被判空闲"。降级时打印 `usermod -aG input` 提示。
- **resource**：只在"有 pid 的未结束会话"上采样；CPU = jiffies 差 / CLK_TCK / 墙钟差 / ncpu × 100；用 `starttime` 防 PID 复用；任何间隙都重置基准，所以每次切会话后的第一个样本总是 `cpu_pct = 0.0`。
- **disruption**：`dbus-monitor` 文本解析（`string "..."` 第 0 个 = app，第 2 个 = summary）；剪贴板 5s 轮询 `wl-paste` + FNV-1a 前 4KiB 去抖。**`dbus-monitor` 不存在时线程直接 return（不重连）**；解析依赖外部文本格式。
- **goal**：读 `daily_summary` 的**本地今天**；50%/100% 里程碑按 `(日期, goal_key, 里程碑)` 去重；`target_type` 只有 `"class"` 与"其他一律当 all"两种语义；**目标进度不含未结束的会话**，所以通知只能在会话结束后触发。
- **wellbeing**：深夜窗口（支持跨午夜），去重键是 `"{day}:{hour}"` → **窗口内每小时提醒一次**；`hyprlock_command` 用 `split_whitespace` 直接 `Command::new`（**不经 shell**，也不能表达带引号的参数），spawn 后不 wait。

## Schema 迁移

`migrate()` **无版本闸门，每次启动全跑**，所有步骤必须幂等。v1 基础表；v2 `activity_state`/`focused_ms`/`activity_events`/`hourly_summary`/`ai_conversations.complete`；v3 `pid`/`app_resources`；v4 `disruptions`；v5 `goals`；v6 负时长修复 + 按需重建汇总；v7 四个 `date_local` 列 + 回填（只处理 `date_local=''` 的行，时间戳无法解析的行**永远**留空）。

server 侧有一份**手工副本**，加列必须两处同时改。

## 会破坏数据的不变量

1. 时长只能来自 `session_elapsed_ms`；`activity_events.duration_ms` 不是单调的（回拨时为 0）。
2. `end_session` 的重写逻辑会改变会话所属日期/小时桶——改 1000ms 容差等于改历史归属。
3. `focused_session_count` 取决于**结束那一刻**的 `activity_state`：空闲路径先置 `"idle"` 再 `end_session`，所以空闲结束的会话永远不计入 focused。
4. `finalize_orphaned_sessions` **不更新汇总表**，孤儿时长只在 `sessions` 里可见，直到有人重建。
5. `update_session_state` **不是事务性的**（`end_session` 是）。
6. 全进程单锁：锁内做长任务（重建、网络）会丢窗口事件；任何 panic 会让锁中毒，之后所有 `if let Ok(..)` 静默跳过工作。
7. `class` 必须小写；`title` 写 `""` 而不是 NULL。
8. `migrate()` 里 `add_column_if_missing` 用字符串拼接表名/列名——只传字面量。
9. 备份必须包含 `hyprtrace.db-wal` / `-shm`。

## 测试

```bash
cargo test -p hyprtrace-daemon        # 47 passed（main 9 / config 1 / db 10 / idle 9 / resource 8 / goal 8 / input 2）
```

未覆盖：`listener.rs` 全部、`disruption_monitor.rs`（解析/重连/哈希）、`wellbeing_monitor.rs`、`notify.rs`、孤儿收尾对汇总的影响、时钟跳变的 `started_at` 重写路径、以及任何需要 evdev 权限/D-Bus/Hyprland/Hyprlock 的行为。`config.rs` 的测试会改进程级 `XDG_CONFIG_HOME`/`HOME`，再加同类测试可能产生并行抖动。
