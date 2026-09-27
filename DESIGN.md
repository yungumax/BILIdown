# 视觉设计说明

改界面之前先读这份，避免每次重新发明一套颜色和规矩。

## 一、设计出发点

BILIdown 是 B 站下载工具，视觉语言直接从主题里取，不用通用模板：

- **B 站双色**：粉 `#FB7299`（品牌）＋ 经典蓝 `#00A1D6`（功能）。
- **弹幕轨迹**：B 站最有辨识度的东西是横穿屏幕的弹幕。把它抽象成背景里
  一层极淡的平行细线（115°），作为全站唯一的装饰性元素。

## 二、两色分工（重要，别混用）

| 色 | 用在哪 | 不用在哪 |
| --- | --- | --- |
| 粉 `--accent` | 品牌与主操作：主按钮、激活导航、当前步骤、选中轨、强调文字 | 数据/进度 |
| 蓝 `--accent-2` | **只用于数据与进度**：进度条、队列忙态、已完成步骤、`已加载 N / M` 计数 | 主操作按钮、导航激活态 |

两色都来自 B 站本身，不是为了凑对比而找的第二个颜色。混用会让界面失去主次。

## 三、令牌

改观感优先改令牌（`src/styles.css` 顶部三处：浅色、深色、跟随系统兜底），
页面里的 scoped 样式大多引用令牌，改一处全站生效。

- **形状**：`--radius-sm 10` / `--radius 14` / `--radius-lg 18`。层级越高圆角越大，
  别让所有元素同一个圆角。
- **层次**：`--shadow-1`（卡片）、`--shadow-2`（浮层）、`--shadow-accent`（主按钮光晕）、
  `--card-highlight`（深色下的顶部高光）。
- **轨道**：`--rail`（粉→蓝渐变，竖向）用于侧栏与分组行；`--flow`（蓝→粉，横向）用于进度。
- **背景场**：`--signal-line` 控制弹幕细线的透明度。深色约 5%，浅色约 4%；
  想更安静就调小，别直接删掉这段背景（它是这套设计的签名）。
- **动效**：`--motion-fast 130ms` / `--motion 220ms` / `--ease-out` / `--ease-spring`。
  统一在 `prefers-reduced-motion` 里一次性关闭。

## 四、排版不变原则

界面改版必须**几何不变**：只动配色、圆角、阴影、字距、层级，不动宽高、间距与栅格。

验证方式（已提交，可复用）：

```bash
# 改之前
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" ./target/release/bilidown.exe
node tools/ui-geometry.mjs geometry-before.json
# 改之后
node tools/ui-geometry.mjs geometry-after.json
# 逐项对比：必须"无差异"
```

基准：`tools/geometry-before.json`（截至 `ui-v2`）与 `tools/geometry-v3*.json`。

## 五、覆盖 scoped 样式的正确姿势

页面组件的 `<style scoped>` 会带 `[data-v-*]` 属性选择器，特异性比同名的全局规则高。
要覆盖它们，全局规则必须加 `body` 前缀（例如 `body .content .card { ... }`），
否则写了也不生效。

## 六、版本标签

| 标签 | 内容 |
| --- | --- |
| `ui-v1` | 动效完成版（改版前的基线） |
| `ui-v2` | 设计第一版：大圆角、柔和投影、主色渐变、侧栏主色条 |
| `ui-v3` | 设计第二版（当前）：B 站双色分工、弹幕轨迹场、轨道系统 |

回退：`git checkout ui-v1 -- src/`
