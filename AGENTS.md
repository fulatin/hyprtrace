# AGENTS.md — HyprTrace

Hyprland 窗口时间追踪器：Rust daemon + Axum server + React SPA，三者共享一个 SQLite（WAL）文件。

## 先读哪个技能

深参考按领域拆成了仓库内技能（`.dsh/skills/`，随仓库分发，改代码前按需加载）：

| 技能 | 什么时候用 |
|---|---|
| `hyprtrace-architecture` | 动手前的地图：组件边界、数据流、目录对照、时间/日期语义、schema 归属、不变量、"改 X 动哪里" |
| `hyprtrace-daemon` | 守护进程、会话生命周期、空闲/资源/打断监控、时长与专注时间语义 |
| `hyprtrace-server` | `/api` 端点、SQL 层、派生指标公式、鉴权、后台任务、AI agent 工具 |
| `hyprtrace-web` | 前端页面/组件/主题/动效、API 客户端、构建与视觉验证 |
| `hyprtrace-ops` | 构建、隔离实例、部署到 9420、systemd、CI 门禁、AUR 打包 |

## 不可协商的约束

1. **不要动正在跑的实例。** 用户级 systemd 服务（`hyprtrace-daemon` / `hyprtrace-server`）通常在服务 `http://localhost:9420` 并有真实数据。不要 `pkill`，不要随手重启；重启走 `systemctl --user restart hyprtrace-server.service`。
2. **不要对同一个数据库起第二个 daemon。** 它不解析参数，会共用同一 config 与 db，双写会话并往桌面弹提醒。要验证请用数据库副本 + 隔离 `HOME`（步骤见 `hyprtrace-ops`）。
3. **`~/.config/hyprtrace/config.toml` 里有真实的 `[ai.openai].api_key`。** 不要 cat、不要复制进仓库/临时目录/命令行，派生临时配置时必须删掉该键。
4. **schema 改动必须落在两处**：`crates/hyprtrace-daemon/src/db.rs` 与 `crates/hyprtrace-server/src/db.rs` 的迁移副本（注释 `KEEP IN SYNC`），并保持 `scripts/repair-negative-durations.sh` 的列预期。
5. **前端不得硬编码颜色**（`gray-*`/`cyan-*`/`#hex`…），只能用 `web/DESIGN.md` 里的语义 token；否则浅色主题会坏。
6. 推之前本地跑一遍 CI 的三步（见下）；`tsconfig` 开了 `noUnusedLocals`，未使用的 import 会直接让构建失败。

## 常用命令

```bash
cargo check --workspace && cargo test --workspace        # CI 前两步（daemon 47 + server 58）
cd web && npx tsc --noEmit -p tsconfig.json              # 前端类型门
cd web && npm run build                                  # 产物在 web/dist（需部署才生效）
curl -s localhost:9420/api/health                        # 运行中的实例
cd web && VITE_API_TARGET=http://127.0.0.1:9421 npm run dev   # 指向临时实例的开发服务器
```

## 本仓库的工作习惯

- 前端**视觉/动效改动必须真的看一眼**（起 dev server + 浏览器截图核对），类型检查通过不等于没坏。
- 改了 `web/` 想让运行中的站点生效，必须重新拷贝到 `~/.local/share/hyprtrace/web`（该路径在工作区之外，需要一次沙箱提权）并确认 `dist/assets` 与目标目录一致。
- 大改动前后用 `git status` 确认没有把 `.aur/`、`.dsh-diag/`、`target/`、`web/dist/` 之类提交进去（`.gitignore` 已覆盖，别用 `git add -f`）。
- 后端新逻辑优先写成 `insights.rs` 那样的**纯函数 + 单测**，SQL 只负责取数。
