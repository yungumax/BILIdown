# tools/ — 启动期与主题的真实运行观测

这些脚本是「窗口生命周期 / 主题闪烁」这类问题的验收手段。它们存在的理由：
这类缺陷**在浏览器里预览完全看不出来**——浏览器没有原生窗口、没有 WebView2 的
初始化脚本时序、没有真实的系统主题。必须在真实 exe 上观测。

## 为什么不能只看代码或浏览器

2026-09-26 那轮「双击 exe 闪烁」的排查结论（都由这些脚本给出，不是推理）：

- 初始化脚本 `document.documentElement.dataset.theme = '...'` 在 WebView2 里
  抛 `TypeError`——脚本执行时 `documentElement` 还是 `null`，属性永远落不上。
  页面于是先用浅色画出来，约 0.9 秒后才被设置接口补上，整页可见地翻一次主题。
- 翻转的 0.9 秒延迟来自 `app_settings`：它每次都 spawn ffmpeg 做版本探测
  （实测 811ms），而前端要等它返回才知道主题。
- 原生窗口底色只在 `system` 模式设置，显式浅/深色时窗口是一块 WebView2 的白色。

## 脚本

前置：应用需要带调试端口启动。

```bash
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" ./target/release/bilidown.exe
```

| 脚本 | 回答的问题 |
| --- | --- |
| `trace-window.ps1` | 启动瞬间（15ms 采样）创建了几个顶层窗口、何时由隐藏变可见、尺寸/位置怎么变。用于判定"是否出现了第二个实例的窗口"。 |
| `cdp-probe.mjs` | 启动全程的 DOM 时间线 + 控制台异常 + 逐帧截图。用于看首帧配色、有没有未捕获异常。自开进程，启动应用前先跑它。 |
| `cdp-visibility.mjs` | **最有用**：在文档创建前注入探针，把「`data-theme` 何时写入」「首帧背景色」「窗口何时可见」「DOM 何时填充」记在**同一条时钟**上，然后 reload 复现一次干净加载。用于判定"先绘制还是先显示""首帧是什么颜色"。 |
| `cdp-toggle.mjs` | 真实点击右上角主题按钮，录下主题变化与 toast 文本。用于区分"主题切换本身有问题"和"启动期的问题"。 |
| `watch-window-events.ps1` | **事件驱动，不漏毫秒级闪现**：`SetWinEventHook` 监听全系统顶层窗口的 CREATE/SHOW/HIDE/DESTROY/FOREGROUND。15ms 轮询抓不到的东西（一闪而过的窗口）全靠它。 |
| `capture-startup.ps1` | 用 `PrintWindow(PW_RENDERFULLCONTENT)` 逐帧抓应用窗口，看用户实际看到的画面。 |

## 2026-09-26 第二轮：真正"弹"的是一个终端窗口

用 `watch-window-events.ps1` 抓到的（轮询、逐帧截图、DOM 探针都漏了这个）：

```
1539 SHOW    cls=CASCADIA_HOSTING_WINDOW_CLASS title='Windows Terminal'
1651 HIDE    ... title='C:\Users\87845\AppData\Local\Programs\ffmpeg\bin\ffmpeg.exe'
1914 SHOW    ... （第二次）
```

程序是 GUI 子系统（`windows_subsystem = "windows"`），但 spawn 控制台程序
（`ffmpeg -version`）时不抑制控制台，Windows 就会为它新建一个终端窗口——
**每次 spawn 弹一个终端一闪而过**；合成视频时终端会一直挂着。
修法：`creation_flags(CREATE_NO_WINDOW)`；另外 `path_ffmpeg` 改为纯 PATH 文件系统
查找，去掉了一次纯粹为了"试试能不能跑"的多余 spawn（探测 811ms → 68ms）。

**教训**：GUI 程序 spawn 控制台子进程必须 `CREATE_NO_WINDOW`。这类缺陷在
DOM 层、窗口几何层、应用窗口截图层全都不可见——子进程的窗口属于另一个进程。

`cdp-visibility.mjs` 依赖 `Page.addScriptToEvaluateOnNewDocument` + reload：
**初始化脚本在首次加载与 reload 时行为一致**，所以 reload 可以复现真实加载。
注意它烘焙的是**进程启动时**读到的主题（初始化脚本在 Rust 侧一次性生成），
所以换主题后再测，必须重启进程。

## 踩过的坑

- **不要用 GDI 截屏（`Graphics.CopyFromScreen`）采 WebView2 的颜色**：
  WebView2 走 GPU 合成，GDI 抓到的是纯黑，会得出"窗口是黑的"这种错误结论。
  要看画面就用 `Page.captureScreenshot`。
- **跨进程时钟对不上**：PowerShell 启动 + `Add-Type` 编译比 Node 慢几百毫秒，
  两个脚本各自的 `t=0` 不是同一时刻，跨脚本比较时间线会得出相反结论。
  要么把观测放进同一个进程，要么让页面自己采集（`cdp-visibility.mjs` 的做法）。
- **`.ps1` 里的中文**：无 BOM 时 Windows PowerShell 按 ANSI 解码，中文注释会
  破坏语法。本目录的 `.ps1` 一律保持 ASCII。
- **`Runtime.enable` 会重放页面生命周期内最后那次未捕获异常**，别误当成"刚发生"。

## 主题菜单的回归测试

`test-theme-menu.mjs` 覆盖：点按钮展开、菜单列出三项且标出当前项、选一项后
主题生效并收起、点击外部与 Esc 收起、全程无 toast、无控制台错误。
需要应用带 `--remote-debugging-port=9222` 启动。

## 命名模板的回归测试

`test-naming-ui.mjs` 覆盖：魔法变量面板能打开、项数与后端 `naming_variables`
一致、点击变量插入到模板、选预设后界面预览与后端 `preview_naming` 渲染**逐字一致**。
面板由后端清单生成，所以「界面列了、后端不认」的变量不可能再出现
（Rust 侧另有 `naming::tests::all_documented_variables_render` 守住这条）。

## 媒体页优先顺序的回归测试

`test-prefs-save.mjs` 覆盖闭环：界面改顺序 → 点保存 → 从后端读回，确认顺序与编码
真的落盘（不是只改了界面）。`test-prefs-ui.mjs` 覆盖并排布局与添加/上移/下移。
注意两个脚本都会改动设置页草稿，跑完重载页面即可丢弃未保存的改动。

## 批量来源分页的联网验证

`live_paging_multi_page` 断言 `loaded` 超过单页条数——单页条数是固定的
（收藏夹 20、UP 空间 30、合集 100），所以超过它只可能是真的翻了页。

```bash
BILIDOWN_TEST_SPACE_MID=946974 \
BILIDOWN_TEST_COLLECTION_URL="https://space.bilibili.com/927587/lists/108434" \
  cargo test -p bilidown -- --ignored --nocapture live_paging_multi_page
```

2026-09-26 的实测结果：

```
fav   loaded=129 total=130 → 至少翻了 7 页
space loaded=300 total=932 → 至少翻了 10 页   （300 是设定的单次上限）
coll  loaded=129 total=129 → 至少翻了 2 页
```

踩坑：`seasons_series_list` 的 `page_size` 超过 20 会返回 `-400 请求错误`，
别误当成风控或"该 UP 没有合集"。

## 界面细节的回归测试

`test-ui-polish.mjs` 覆盖四项：单个视频模式不显示支持来源行、勾选框为自绘的
粉底白勾 16×16（`appearance:none`）、提示图标是圆圈问号、每个提示图标都有
tooltip 文本。

踩坑：解析页原来有一条 `textarea, input { width: 100% }`，把批量列表里的勾选框
也拉成了整行宽（30×26）。设置页早就是 `input:not([type="checkbox"])`，解析页漏了。
写宽泛的 input 规则时记得排除 checkbox。

## 「选择内容」独立页与增量加载

`test-select-page.mjs` 覆盖：解析后进入独立页、首屏只加载第一页、顶部显示
「已加载 N / M 项」、点「继续解析」行数真的变多且顶部数字同步、全选已加载、
取消勾选后「下载所选 (N)」计数跟着变。

`live_incremental_loading` 是它的后端对应物（联网）：断言首页只给 20 条
（收藏夹每页 20），「继续解析」至少再取 50 条，两次之间不重复、进度等于两次之和。

```bash
cargo test -p bilidown -- --ignored --nocapture live_incremental_loading
```

踩坑：批量来源列表项是 Vue 深层响应式对象，`probe.items.push(...)` 能直接触发更新，
但 `loaded/total/exhausted/note` 要逐个赋值（不是 computed）。

## 单视频也进「选择内容」页

`test-multi-source.mjs` 覆盖：一次贴两行（视频 + 收藏夹）解析后出现来源切换标签，
第 1 个来源是视频时显示详情块（封面/标题/时长/BV号 + 清晰度音轨 + 加入下载），
切到收藏夹时换成表格，切回来又变详情。

`test-select-page.mjs` 覆盖批量来源那条路径（表格、分批加载、勾选、下载所选）。

## 「选择内容」页的滚动行为

`test-scroll.mjs` 断言：外层内容区不可滚（scrollHeight == clientHeight）、表体可滚、
滚表体后顶部工具条与底部统计的位置不变。曾经的问题是整页在滚——工具条和底部
统计会跟着走，现在只有中间的表体滚。

## 命名预设下拉与步骤条

`probe-naming.mjs` 覆盖：选预设 → 模板填入且下拉显示预设名；手改模板 → 下拉立刻
变「自定义模板」（不再是停留在旧预设名上骗人）；保存后依然一致。

步骤条（解析来源 / 选择内容）可点击跳转，当前步不可点。

## 下载设置弹层与步骤条状态

`test-dl-panel.mjs` 覆盖：工具条按钮顺序（继续解析 / 下载设置 / 下载全部 / 下载所选）、
底部只剩统计与全选、弹层含清晰度与音轨且不超出视口、步骤条状态跟着当前页面走
（输入页：第 1 步 active 有横线、第 2 步 idle；选择页：第 1 步 done 无横线、第 2 步 active 有横线）。
