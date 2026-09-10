---
name: hyprtrace-web
description: hyprtrace-web 前端参考：页面/组件/数据层结构、设计系统（CSS 变量主题 token、组件类、基础件 API）、framer-motion 动效约定、路由过渡与滚动显现的已知坑、类型与 API 客户端契约，以及构建与验证方式。
whenToUse: 修改任何前端页面/组件/样式/动效、加图表、改主题、改 API 客户端、排查"页面空白/动画不动/样式在浅色下坏掉"时。
---

# hyprtrace-web 参考

React 18 + TypeScript + Vite 5 + Tailwind 3 + Recharts 2 + framer-motion 11。**先读 `web/DESIGN.md`**——它是约束文档（"以下都已存在，复用，不要另造一套"），不是装饰。

## 结构

```
src/main.tsx            挂载 + StrictMode
src/App.tsx             ThemeProvider → BrowserRouter → ErrorBoundary → Layout → 8 条路由
src/components/Layout.tsx   侧栏（可折叠，layoutId 高亮滑块）+ 顶栏 + 路由过渡 + 滚动复位
src/lib/api.ts          fetchJSON + 全部 REST（含 api.insights.* 命名空间）；401 清 token
src/lib/types.ts        与 server models.rs 一一对应的类型（字段名是契约）
src/lib/transport.ts    /api/ai/chat/agent 的 NDJSON → AI SDK 消息流（只认 5 种 type）
src/lib/auth.ts         X-Auth-Token（localStorage: hyprtrace_auth_token）
src/lib/theme.tsx       深色/浅色/跟随系统（localStorage: hyprtrace.theme）
src/lib/motion.ts       动效预设
src/lib/format.ts       时长/百分比/内存/相对日期格式化（formatDuration / formatSpan / formatDelta / formatCount…）
src/components/ui/      Card+Panel / Reveal(+RevealItem/RevealOnScroll) / Feedback(Skeleton/EmptyState/ProgressBar)
                        / CountUp / Sparkline / SegmentedControl / ThemeToggle
src/components/insights/  TransitionMatrix / FlowDiagram / ForecastChart / RhythmHeatmap
                        / FragmentationPanel / CooccurrenceGraph / DisruptionImpact / AnomalyList
src/components/         StatCard / ActivityHeatmap / AppUsagePie / HourlyBars / AppRankingBar
                        / AppTrendChart / AppName / ErrorState / ErrorBoundary（其余为 AI 聊天专用）
src/pages/              Dashboard, Insights, Apps, Timeline, Sessions, Titles, AIChat, Settings
```

## 设计系统（要点）

- 颜色**只能**用语义 token：`bg-bg` `bg-surface` `bg-surface-2/3` `border-line(-strong)` `text-fg(-muted/-faint)` `text-accent(-2/-3)` `text-good/warn/bad`。**禁止** `gray-*` `cyan-*` `slate-*` `white` `black` `#hex`（数据系列色板除外）。
- token 是 `index.css` 里的 CSS 变量（`:root/.dark` 与 `.light` 两套），Tailwind 用 `rgb(var(--c-*) / <alpha-value>)` 暴露，所以 `bg-surface/60` 这类透明度修饰符在两种主题下都成立。
- 注意：**只有 5 的倍数的透明度档位会被生成**（`/10 /15 /25 …`）。`bg-accent/12` 不会生成 CSS，会静默失效——曾出现过这个 bug。
- 组件类：`.card` `.card-pad` `.card-interactive` `.btn(-accent/-ghost)` `.input` `.chip(-neutral/-accent/-good/-bad/-warn)` `.panel-title` `.panel-sub` `.divider` `.skeleton` `.tnum`（等宽数字）`.dot-grid` `.text-gradient`。
- 主题切换靠 `<html>` 上的 `.dark`/`.light` 类 + `index.html` 里的首屏内联脚本（防闪烁，读同一个 localStorage 键）。

## 动效约定

- 预设都在 `lib/motion.ts`：`spring` `softSpring` `easeOut` `staggerContainer(n)` `fadeUp` `fadeIn` `scaleIn` `pageTransition` `listItem` `expand` `hoverLift` `barTransition`。新动效优先复用，不要另加时长/曲线。
- 时长 0.2–0.45s；**所有大动作都要尊重 `useReducedMotion()`**。
- 数据驱动的宽度/高度/位置必须用 `motion.div` 的 `initial/animate`，不要用 CSS 类，否则数值更新会跳变。
- Recharts：`isAnimationActive` + `animationDuration≈700-900` + `animationEasing="ease-out"`；tooltip 颜色用 `rgb(var(--c-*))` 字符串。
- 列表增删用 `AnimatePresence` + `layout`；但**页面级路由过渡只能有入场动画**（见下）。

## 已修复的坑（别改回去）

1. **路由过渡只做入场**。`Layout` 里若用 `<AnimatePresence mode="wait">` 包 `<Outlet/>` 并配 `exit`：退场期间旧容器会因为 `Outlet` 读实时 router context 而渲染**新**页面（重复挂载），且页面内部的嵌套 `AnimatePresence` 一旦在飞行中，退场可能永远不完成 → 内容区空白直到手动刷新。`pageTransition` 因此没有 `exit`。
2. **换页要复位滚动**：`Layout` 用 `useLayoutEffect` 在 pathname 变化时把 `main` 的 scrollTop 置 0；否则会停在上一个页面的滚动位置，上半页的滚动显现元素永远不触发。
3. **`RevealOnScroll` 用显式 IntersectionObserver**，并且在挂载时若元素已在视口内（或上方）立即显示——`whileInView` 只对"进入视口"有反应，刷新恢复滚动位置时会留下永久透明的区块。
4. **年度热力图**（`ActivityHeatmap`）：`@uiw/react-heat-map` 的 `<svg>` 没有宽度属性时会退回 300px 固有宽度，只渲染约 17/53 周 → 必须传 `style={{ width: '100%' }}`；另外该库取色是"第一个严格大于 count 的档位"，会让 0 分钟的日子也显示成有活动 → 颜色由 `rectRender` 自己按分钟数计算。同时用 ResizeObserver 按容器宽度算 `rectSize`。
5. **Sessions 表格**：7 列在 1280px 下会溢出，现在是 `table-fixed` + 固定列宽 + `overflow-x-auto`，`Focus` 列在 `2xl` 以下隐藏；应用名靠 `min-w-0` + `truncate` 截断（否则会糊到 Title 列上）。
6. `Layout` 侧栏高亮用的是 `bg-accent/10`——改透明度档位要检查是否真的生成了 CSS。

## 数据层契约

- 所有请求经 `fetchJSON`：自动带 `X-Auth-Token`、15s 超时、401 清 token。
- **后端多数错误是 HTTP 200 + `{"error":...}`**，`fetchJSON` 不会抛，会在组件里才炸——新增调用点要考虑这点。
- 洞察页用 `api.insights.*`；`forecast` **没有 from/to**（服务端固定到今天）。
- AI 对话走 `lib/transport.ts`，只认 NDJSON 的 `text` / `tool_call` / `tool_result` / `done` / `error`。
- 时间戳是 UTC RFC3339，展示前用 `date-fns` 转本地；日期字符串（`date_local` 系列）本来就是本地日期，不要再用 `new Date()` 当作 UTC 解析。

## 构建与验证

```bash
cd web
npx tsc --noEmit -p tsconfig.json      # 类型门；strict + noUnusedLocals/Parameters，未使用的 import 会直接失败
npm run build                          # tsc -b && vite build → dist/
VITE_API_TARGET=http://127.0.0.1:9421 npm run dev   # 开发服务器（默认代理到 9420）
```

- `vite.config.ts` 有 `manualChunks` 分包（react / charts / motion / markdown），别再合成一个 1.5MB 的包。
- 视觉改动**必须真的看一眼**：起 dev server 后用浏览器打开并截图核对，不要只靠类型检查通过就交付。注意本仓库的滚动容器是 `main`，不是 window——外层滚动截图/滚动脚本可能不生效。
- 构建产物要生效必须重新部署（见 `hyprtrace-ops`），只 `npm run build` 不会更新正在服务的页面。
