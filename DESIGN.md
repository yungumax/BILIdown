# 视觉设计说明

改界面之前先读这份。当前版本：`ui-v5`（回到 `ui-v1` 的平面化风格并在此基础上优化）。

## 一、定位

BILIdown 是工具型界面（Operate 场景）：可扫读、一致、符合桌面直觉优先于表达欲。
品牌感放在精确的细节里，不做装饰。

## 二、设计语言：平面化

**层次靠"面色分层 + 发丝线"，不用投影、渐变、玻璃、色带。** 这是本项目的定调，
换版本时也不要往回加。

- 面色三档：底 → 卡片 → 抬起，靠明度差拉开，不靠阴影。
- 分隔一律 1px 发丝线（`--line` / `--line-soft`）；表格行分隔用
  `box-shadow: inset 0 -1px 0`，这样不占排版。
- 禁止：卡片投影、卡片渐变、`backdrop-filter` 模糊、>1px 的彩色侧边条、
  装饰性纹理（曾试过"弹幕轨迹背景 + 粉/蓝双色分工"= `ui-v3`，已否决）。
- 主色 `#fb7299` 是唯一强调色；状态用**平色染面**表达（导航激活、选中行、标签）。

## 三、文字对比度（硬规则）

正文、说明、次要、弱化四档文字都必须 ≥4.5:1。粉色当文字色在白底只有 2.6:1，
到不了这条线，所以：

- `--accent`：只用于**图标、描边、面**。
- `--accent-ink`：主色当**文字色**时用它（浅色 `#b52a5c`，深色 `#ff9ec0`）。

浅色文字档位：`--text #24242a`（15.4:1）、`--muted #5f5f6a`（6.3:1）、
`--faint #74747f`（4.6:1）。不要为了"看起来轻"调浅这几档。

## 四、令牌

改观感优先改令牌（`src/styles.css` 顶部三处：浅色、深色、跟随系统兜底）。

- 形状：`--radius-sm 8` / `--radius 10` / `--radius-lg 14`
- 动效：`--motion-fast 130ms` / `--motion 220ms` / `--ease-out` / `--ease-out-expo`
- 浏览器自带界面也要主题化：`::selection`、`caret-color`、`prefers-reduced-motion`、
  键盘聚焦环（`:focus-visible`）、表格数字列等宽 —— 这些是最便宜的"被认真做过"的信号

## 五、动效原则（Operate 场景：动效服务反馈、状态、连续性）

- **一个编排好的时刻**：解析结果出现时上浮一次（`.page-in`）。
- **状态变更要立刻看得见**：悬停/选中的平色过渡 130ms，只动颜色与透明度。
- **不做**：逐行入场（每块套同一套入场）、回弹/弹性缓动（用 `--ease-out-expo`）。
- **不动画布局属性**：`width`/`height`/`padding`/`margin` 一律不用，
  进度条用 `transform: scaleX()`（检测器会把 `transition: width` 判为布局抖动）。
- 统一在 `prefers-reduced-motion` 里一次性关闭。

## 六、排版不变原则

界面改版必须**几何不变**：只动配色、圆角、线条、字重字距、层级，不动宽高、间距与栅格。

```bash
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" ./target/release/bilidown.exe
node tools/ui-geometry.mjs geometry-before.json   # 改之前
node tools/ui-geometry.mjs geometry-after.json    # 改之后（逐项对比，必须"无差异"）
```

想加分隔线就用 `box-shadow`，不要用 `padding` —— 后者会把下面整体推移。

## 七、覆盖 scoped 样式的正确姿势

页面组件的 `<style scoped>` 带 `[data-v-*]` 属性选择器，特异性比同名全局规则高。
全局覆盖必须加 `body` 前缀（如 `body .content .card { ... }`），否则写了不生效。

## 八、反模式检测（技能引擎）

```bash
node D:/Zcode/toolchain/npm-global/node_modules/impeccable/cli/bin/cli.js detect src/
```

**提交前跑一次，必须 0 反模式。** 引擎来自 npm 包 `impeccable@4.1.0`
（装在 `D:\Zcode\toolchain\npm-global`），同目录 `impeccable.exe` 是把它包成原生
exe 的 shim（技能启动器只认 exe），用户级环境变量 `IMPECCABLE_BIN` 指向它。
检测器已抓出并修掉的两类问题：`transition: width`（改 `transform: scaleX()`）、
回弹缓动（改指数缓出）。

## 九、版本标签

| 标签 | 内容 |
| --- | --- |
| `ui-v1` | 原始平面风格 + 动效层（**当前方向的基线**） |
| `ui-v2` | 大圆角 + 柔和投影 + 主色渐变（已放弃） |
| `ui-v3` | 弹幕轨迹背景 + 粉/蓝双色分工（已否决） |
| `ui-v4` | 在 v2 基础上的精修（已放弃） |
| `ui-v5` | 当前版：回到 v1 平面化并优化（对比度达标、发丝行线、平色状态、浏览器界面主题化） |

回退：`git checkout ui-v1 -- src/styles.css`（只回样式，不影响后续功能）
