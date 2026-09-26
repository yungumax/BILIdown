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
