---
name: hyprtrace-ops
description: HyprTrace 构建、运行、部署与打包操作手册：本地编译/测试、用数据库副本起隔离实例（含必须关掉的后台副作用）、把前端部署到正在服务的 9420 目录、用户级 systemd 服务、CI 门禁、AUR 打包，以及绝对不能做的操作清单。
whenToUse: 需要构建、运行、重启、部署、调试运行中的实例、起临时实例验证接口、推 CI 前自查、或处理打包/服务问题时。
---

# HyprTrace 运维手册

## 构建与测试

```bash
cargo build --release                # 两个二进制：target/release/hyprtrace-{daemon,server}
cargo check --workspace              # CI 第一步
cargo test --workspace               # CI 第二步（daemon 47 + server 58）
cd web && npx tsc --noEmit -p tsconfig.json && npm run build   # CI 第三步（npm ci 更严格）
```

两个二进制**都不解析命令行参数**（没有 clap），全部配置来自 config.toml 与环境变量。

## 路径与端口

| 项 | 位置 |
|---|---|
| 配置文件 | `$XDG_CONFIG_HOME/hyprtrace/config.toml`（否则 `~/.config/hyprtrace/config.toml`）；**daemon 首次运行会创建**，server 不会 |
| 数据库 | `[daemon] db_path`，默认 `~/.local/share/hyprtrace/hyprtrace.db`（WAL；备份要带 `-wal`/`-shm`） |
| HTTP | `[server] host/port`，默认 `127.0.0.1:9420` |
| 前端静态目录 | `resolve_web_dir()`：优先 `/usr/share/hyprtrace/web`（若 index.html 存在），否则 `$HOME/.local/share/hyprtrace/web`；**启动时解析一次** |
| 周报输出 | `~/.local/share/hyprtrace/reports/weekly-<date>.md` |

`~` 展开用 `$HOME`（未设时字面量 `/root`），**不认 XDG_DATA_HOME**。

## 起隔离实例（验证接口/改动，不碰用户数据）

没有 `--config`/`--port` 参数，只能靠 `HOME` + `XDG_CONFIG_HOME` + 复制出来的 config：

```bash
ISO=/tmp/hyprtrace-iso; mkdir -p "$ISO/config/hyprtrace" "$ISO/home"
# 从真实配置派生时务必去掉 [ai.openai].api_key（里面有真密钥，别把它带进临时目录或日志）
cp ~/.config/hyprtrace/config.toml "$ISO/config/hyprtrace/config.toml"
sqlite3 ~/.local/share/hyprtrace/hyprtrace.db ".backup '$ISO/copy.db'"   # WAL 一致性快照；不要裸 cp
# 编辑副本：db_path 用绝对路径；[server] port = 9421；
#           weekly_report_enabled = false；retention_days = 0
HOME="$ISO/home" XDG_CONFIG_HOME="$ISO/config" ./target/release/hyprtrace-server
```

注意点：

- **PROACTIVE_AI 无法关闭**（间隔会被 `max(.., 15min)` 抬起）→ 临时实例必须隔离 `HOME`，否则会**花 API 额度并往桌面发通知**。
- `retention_days > 0` 会**真删数据**；`weekly_report_enabled` 会写文件并通知。
- server **不会**创建 db 的父目录，先 `mkdir -p`。
- 起后台任务时用受管的后台作业（不要 `&` 后不管），否则进程会随命令结束被回收；停止用 `fuser -k 9421/tcp`，**不要用 `pkill -f`**（会连自己的 shell 一起杀）。
- 前端指向临时实例：`cd web && VITE_API_TARGET=http://127.0.0.1:9421 npm run dev`（Origin 白名单接受任意回环端口，无需 CORS 配置）。

## 部署前端到正在服务的实例

`~/.local/share/hyprtrace/web` 是 install.sh **拷贝**出来的目录，不是源码；只 `npm run build` 不会更新页面。

```bash
cd web && npm run build
rm -rf ~/.local/share/hyprtrace/web/assets && cp -r web/dist/. ~/.local/share/hyprtrace/web/
```

该路径在会话工作区之外，`workspace-write` 策略下第一次写入会被沙箱拒绝——按提示**用同一条命令重试并附一句理由**申请提权即可（用户会看到授权弹窗）。部署后核对 `dist/assets` 与目标目录文件名一致。

## 服务与重启

用户级 systemd 单元：`hyprtrace-daemon.service`（`graphical-session.target`，`Restart=on-failure`）、`hyprtrace-server.service`（`After=network.target`）。ExecStart 指向 `%h/.local/bin/hyprtrace-{daemon,server}`。

```bash
systemctl --user status|restart hyprtrace-server.service
journalctl --user -u hyprtrace-server -n 50
```

- **agent 沙箱里 `systemctl --user` 通常连不上用户总线**（`Failed to connect to user scope bus`）。此时 `is-active` 返回 1 **不代表服务挂了**，也不代表你可以放弃——换用 `danger-full-access` 提权重试同一条命令（实测可行），或让用户在自己的终端执行。
- 替换正在运行的二进制会 `ETXTBSY`：先 `cp` 成新名字再 `mv -f` 覆盖，然后重启服务。
- 服务重启后 `/api/health` 应返回 `{"status":"ok",...}`；旧的静态资源哈希变化说明前端已更新。

## CI 门禁（推之前本地跑一遍）

`.github/workflows/ci.yml`：`cargo check --workspace` → `cargo test --workspace` → `cd web && npm ci && npm run build`（Node 20，`tsc -b` 含严格类型门）。**没有** clippy / fmt / lint 步骤，但以上任一失败就会红。注意：`rusqlite` 未开 `bundled`，链接依赖系统 libsqlite3（pkg-config）。

## 打包（AUR）

`.aur/` 在 `.gitignore` 里，**不要提交**。`PKGBUILD` 的 `source` 未固定（跟随默认分支），`pkgver()` 在无 tag 时生成 `0.1.0.r<count>.<short-sha>`；改 `PKGBUILD` 后必须 `makepkg --printsrcinfo > .SRCINFO`。系统安装在 `/usr/bin` + `/usr/share/hyprtrace/web` + `/usr/lib/systemd/user`，因为 `resolve_web_dir` 优先 `/usr/share`，**装了 AUR 包会静默覆盖本地开发版前端**。

## 绝对不要做

1. 不要 `pkill hyprtrace-server`/随便重启——那是用户正在用的实例；要重启就走 systemd，并在做之前想清楚影响。
2. **不要用真实配置起第二个 daemon**：它不解析参数、共用同一 config 与 db，会双写会话并在桌面弹提醒。
3. 不要把 `~/.local/share/hyprtrace/web` 当源码改；也不要删掉它不管（会让运行中的服务 404）。
4. 不要 `cat`/复制/打印 `~/.config/hyprtrace/config.toml`——里面有真实的 `[ai.openai].api_key`。
5. 不要删 `web/node_modules` 或 `web/package-lock.json`；`npm ci` 需要网络（npm 缓存默认在 `~/.npm`，沙箱内可能需要 `--cache` 指到工作区内）。
6. 不要把 `.aur/`、`.dsh-diag/`、`target/`、`web/dist/`、`node_modules/` 提交进仓库。
7. 改 schema 只改一处：daemon 与 server **各有一份迁移**，`scripts/repair-negative-durations.sh` 也依赖列名。
