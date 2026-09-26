# BDL 软件逆向分析 & BILIdown 实施方案

> 分析日期：2026-09-26 ｜ 分析对象：`E:\02GJ\BDL\bdl-desktop.exe`（桌面快捷方式 BDL.lnk 指向）

---

## 一、BDL 软件分析结论

### 1.1 基本信息

| 项目 | 结果 |
|---|---|
| 软件名 | BDL，版本 0.6.0，作者 yueli |
| 应用标识 | `com.yueli.bdl` |
| 技术栈 | **Tauri（Rust 后端 + 系统 WebView2 前端壳）**，单 exe 约 28MB，免安装 |
| 证据 | 二进制中 `tauri` 命中 256 次、`WebView2` 命中 10 次；本地存在 `EBWebView` 用户数据目录（Edge WebView2 特征）；无 wails/electron/flutter 特征 |
| 数据存储 | `%APPDATA%\com.yueli.bdl\settings.json`（设置）+ `tasks.sqlite`（任务队列/下载记录，SQLite + WAL 模式） |
| 外部依赖 | 内置/调用 **ffmpeg** 做音视频合成（二进制中 ffmpeg 相关字符串 12 处） |

### 1.2 功能范围（由二进制内 B 站 API 字符串反推）

BDL 的功能面可以从它调用的接口完整还原：

**支持的内容类型：**
1. **普通视频** — `x/web-interface/view`（视频信息）+ `x/player/wbi/playurl`（播放地址，带 **wbi 签名**）
2. **番剧/影视** — `pgc/view/web/season`（剧集信息）+ `pgc/player/web/playurl`（播放地址）
3. **课程(充电专属)** — `pugv/view/web/season`、`pugv/view/web/ep/list`、`pugv/player/web/playurl`
4. **UP 主投稿批量** — `x/space/wbi/arc/search`（投稿列表）+ `x/polymer/web-space/seasons_archives_list`（**合集**）+ `x/series/archives`、`x/series/series`（**系列**）
5. **收藏夹批量** — `x/v3/fav/folder/created/list-all`（自己创建的收藏夹）、`x/v3/fav/folder/collected/list`（收藏的收藏夹）、`x/v3/fav/resource/list`（收藏夹内容）
6. **URL 智能识别** — 二进制中含 `www.bilibili.com/video/`、`/bangumi/play/ep`、`/cheese/play/ep`、`/cheese/play/ss` 等路径模式

**账号与清晰度：**
- **扫码登录** — `passport.bilibili.com/x/passport-login/web/qrcode/generate` + `/poll`，用 `x/web-interface/nav` 校验登录态
- `SESSDATA` Cookie 管理（二进制命中 6 次，cookie 相关 50 次）→ 支持大会员清晰度（1080P+/4K/HDR/杜比视界/杜比全景声/**Hi-Res FLAC**，均有关键字命中）
- **wbi 签名**（命中 26 次）— 说明其接口调用已适配 B 站当前的反爬签名机制

**下载能力：**
- 弹幕（danmaku 命中 13 次）、字幕（subtitle 32 次）下载
- 音视频分离下载（DASH）后 **ffmpeg 合并**（merge 11 次）

### 1.3 架构启示

- Tauri 单文件、体积小、内存占用低 → 这条技术路线被 BDL 验证完全可行
- SQLite 存任务 → 支持断点续传/历史记录的前提
- 核心复杂度全在 Rust 侧（API 客户端、wbi 签名、下载引擎、ffmpeg 调度），前端只做展示和任务管理

---

## 二、BILIdown 实施方案

### 2.1 技术选型（推荐 + 备选）

**推荐：Tauri 2 + Vue 3 (TypeScript) + Rust**（与 BDL 同路线，已被验证）

| 层 | 选型 | 理由 |
|---|---|---|
| 壳 | Tauri 2 | 内存低，复用系统 WebView2（Win10/11 自带）；程序本体 ~15-30MB，打包 ffmpeg 后安装包约 50MB（见模块④） |
| 后端 | Rust + tokio + reqwest | 异步下载引擎；`serde` 解析 JSON；`md-5` 做 wbi 签名 |
| 数据库 | `rusqlite`（SQLite） | 任务队列/断点续传/下载历史，对标 BDL 的 tasks.sqlite |
| 合成 | ffmpeg（**随程序打包**，Tauri sidecar 外置二进制） | `-c copy` 无损合成，CPU 占用为零，用户开箱即用 |
| 前端 | Vue 3 + Vite + Naive UI/Element Plus | 中文生态好，开发快 |
| 发布 | Git + GitHub Actions + tauri-plugin-updater | 版本管理、发布与自动更新全链路（见 2.4 工程基建） |

**备选路线**（按团队情况选择）：
- **Electron + Node.js**：开发最快、生态最全（可用 fluent-ffmpeg），但安装包 100MB+、内存 300MB+。适合"先跑通再说"。
- **Wails (Go)**：单 exe、Go 比 Rust 上手快，UI 同样用 WebView，折中选项。
- **Python + PySide6**：原型最快，适合纯自用工具，但打包分发体验差。

> 下文按推荐路线展开；若换路线，模块划分和 API 层设计完全通用。

### 2.2 系统架构

```
┌─────────────── 前端 (Vue 3, WebView2) ───────────────┐
│  链接输入/解析预览  清晰度&流选择  任务列表  设置  登录码 │
└──────────────────────┬───────────────────────────────┘
                       │ Tauri IPC (invoke / event)
┌──────────────────────▼───────────── Rust 后端 ────────┐
│ ① URL 解析器     ② Bilibili API 客户端(wbi签名/重试)   │
│ ③ 账号管理(扫码+SESSDATA)  ④ 下载引擎(队列/并发/续传)   │
│ ⑤ 合成模块(ffmpeg/弹幕转ass)  ⑥ 任务持久化(SQLite)      │
└──────────────────────────────────────────────────────┘
```

### 2.3 核心模块设计

#### 模块①：URL 解析器
支持输入格式（正则提取）：
- 普通视频：`BV` 号、`av` 号、完整链接（含 `/video/`、b23.tv 短链需先 HEAD 跟随重定向）
- 番剧：`ss`/`ep` 号
- 课程：`/cheese/play/ss|ep`
- 收藏夹：`space.bilibili.com/{mid}/favlist?fid=xxx`
- UP 主：`space.bilibili.com/{mid}`（拉投稿/合集列表）
- 输出统一为 `enum Target { Video(bvid), Bangumi(ep_id), Cheese(...), FavList(fid, mid), Space(mid), Collection(...) }`

#### 模块②：API 客户端（最关键，难点集中在这）
必须实现的接口清单（与 BDL 一致，均有公开文档）：

| 用途 | 接口 |
|---|---|
| 视频信息 | `GET /x/web-interface/view?bvid=` |
| 播放地址 | `GET /x/player/wbi/playurl`（**wbi 签名**，参数 qn/fnval/platform） |
| 番剧信息/地址 | `GET /pgc/view/web/season`、`/pgc/player/web/playurl` |
| 收藏夹 | `GET /x/v3/fav/resource/list`（翻页 20/页） |
| UP 投稿/合集 | `GET /x/space/wbi/arc/search`、`/x/polymer/web-space/seasons_archives_list` |
| 登录 | `passport /x/passport-login/web/qrcode/generate` + `/poll` |
| 登录态校验 | `GET /x/web-interface/nav`（同时取 wbi 密钥） |
| 弹幕 | `api.bilibili.com/x/v1/dm/list.so?oid=`（protobuf/XML） |
| 字幕 | player 接口返回的 subtitle 列表（JSON） |

**三个必须攻克的反爬点：**
1. **wbi 签名**：从 `nav` 接口返回的 `wbi_img.img_url/sub_url` 取 img_key、sub_key，按固定混淆表（MIXIN_KEY_ENC_TAB）重排拼接成 mixin_key，对 query 参数排序后追加 `wts`（时间戳）做 MD5 得到 `w_rid`。密钥有缓存期，需定时刷新。
2. **请求头伪装**：每次请求必须带 `User-Agent`（真实浏览器 UA）、`Referer: https://www.bilibili.com/`、`Origin: https://www.bilibili.com`，否则 playurl 返回风控页。
3. **Cookie 管理**：SESSDATA + buvid3 + b_nut 等一起携带；登录二维码 poll 返回的 url 参数中含 SESSDATA，需 URL 解码保存。

**清晰度策略**：`fnval=4048`（请求 DASH 全格式），qn 从最高往下 fallback；未登录最高 480P，普通登录 1080P，大会员解锁 4K/HDR/杜比/Hi-Res。

#### 模块③：下载引擎
- 任务状态机：`Pending → Resolving → Downloading → Merging → Done / Failed / Paused`
- DASH 模式：视频流(m4s)与音频流(m4s)分离下载，各自支持 **HTTP Range 断点续传**、分片并发（单文件 4~8 连接）、全局限速、失败重试（指数退避）
- 并发模型：tokio 任务队列，全局并发任务数可配（默认 3），任务内分片并发（默认 4）
- 进度事件通过 Tauri `event.emit` 推给前端（每 500ms 聚合一次，避免 IPC 风暴）

#### 模块④：合成模块（ffmpeg）
```
ffmpeg -i video.m4s -i audio.m4s -c copy -movflags +faststart output.mp4
```
- `-c copy` 直接封装不转码，秒级完成
- 弹幕：XML/protobuf → **ass** 字幕（移植 danmaku2ass 算法：滚动时间、防重叠轨道），播放器可挂载
- 字幕：B 站返回的 JSON 字幕转 **srt**
- 封面、Metadata（标题/UP主/简介）写入 mp4 tag
- **ffmpeg 分发方式（已定：随程序打包）**，采用 Tauri 2 的 sidecar（外置二进制）机制：
  - `tauri.conf.json` 中声明 `bundle.externalBin: ["binaries/ffmpeg"]`，构建前把下载好的 ffmpeg 重命名为带目标三元组的 `binaries/ffmpeg-x86_64-pc-windows-msvc.exe`；安装包内自带，安装后与主程序同目录释放，Rust 侧通过 shell 插件按 sidecar 名称调用，用户无需任何手动安装
  - 构建版选择：使用 **LGPL 静态构建**（推荐 BtbN/FFmpeg-Builds 的 `ffmpeg-master-latest-win64-lgpl.zip`，单文件、无 DLL 依赖）。完整版 ffmpeg.exe 约 80~90MB，经安装包 LZMA 压缩后总体积约 45~60MB；在"关于"页附 FFmpeg 许可声明和源码获取链接即可满足 LGPL 分发义务
  - 后期可选瘦身：自编译裁剪版 ffmpeg（只开 file/http/https 协议、mp4/mkv 封装、aac/ass/srt 编解码），可压到 10~20MB
  - 兜底：保留"检测系统 PATH"逻辑，环境里已有 ffmpeg 时优先用系统的

#### 模块⑤：持久化（SQLite）
```sql
tasks(id, bvid, ep_id, title, owner, quality, codec, audio_codec,
      status, save_path, total_bytes, downloaded_bytes, created_at)
segments(task_id, idx, url, start, end, downloaded, tmp_path)
accounts(cookie_json, updated_at)          -- 密文存储，Windows 用 DPAPI
settings(key, value)                       -- 或沿用 settings.json
```

#### 模块⑥：账号
- 扫码登录（生成二维码 → 轮询 → 存 cookie），对标 BDL
- 兜底：允许用户在浏览器 F12 复制 SESSDATA 手动粘贴

### 2.4 工程基建：Git 仓库、CI/CD 与自动更新

**① Git 仓库**
- 本地 `D:\Zcode\BILIdown` 执行 `git init`，配套 `.gitignore`：`node_modules/`、`src-tauri/target/`、`dist/`、`src-tauri/binaries/`（ffmpeg 体积大不入库，由脚本/CI 下载放置）
- 远程：GitHub 仓库（起步可私有，稳定后转公开），可选 Gitee 作国内镜像双推
- 分支与提交：main 为主干，功能分支开发 + Conventional Commits（feat/fix/docs...）

**② CI/CD（GitHub Actions）**
- 检查流水线（push/PR 触发）：rustfmt + clippy + 前端 tsc/build，保证主干始终可构建
- 发布流水线（推 tag `v*` 触发）：官方 `tauri-action` 在 `windows-latest` 上构建 NSIS 安装包与更新签名文件，自动创建 GitHub Release，上传 `BILIdown_<版本>_x64-setup.exe` 和 `latest.json`

**③ 自动更新（tauri-plugin-updater，Tauri 官方插件）**
- 签名：`tauri signer generate` 生成密钥对，公钥写入 `tauri.conf.json`，私钥存 GitHub Secrets（`TAURI_SIGNING_PRIVATE_KEY`），CI 构建时自动签名更新包
- 应用侧：注册 updater 插件，检查端点指向 GitHub Release 的 `latest.json`；启动后异步静默检查，设置页提供"手动检查更新"
- 更新流程：发现新版 → 弹窗展示版本号与更新说明 → 后台下载并显示进度 → 公钥验签 → 提示重启完成安装；Windows 下由 NSIS 安装包原位替换升级，SQLite 任务记录与设置数据不受影响
- 关键配置：`tauri.conf.json` 中 `bundle.createUpdaterArtifacts: true`、`plugins.updater.endpoints`、`pubkey`
- 国内加速：updater 支持多端点 fallback，设置中可切换 ghproxy 镜像或自建服务器地址，避免大陆环境下载 Release 失败

### 2.5 开发里程碑

| 阶段 | 内容 | 预估 |
|---|---|---|
| **M0 工程起步** | git init + 关联远程仓库 + .gitignore + CI 检查流水线跑通 | 0.5 天 |
| **M1 CLI 原型** | 模块②+④：输入 BV 号 → 下载 360P/720P → ffmpeg 合成出 mp4。**先打通 wbi 签名与请求头这两个最大风险点** | 1~1.5 周 |
| **M2 GUI 骨架** | Tauri 壳 + 链接解析预览页 + 任务列表 + 进度条 | 1 周 |
| **M3 完整下载** | DASH 分离流、断点续传、批量（收藏夹/合集/UP 主翻页）、扫码登录 | 1.5~2 周 |
| **M4 体验完善** | 弹幕/字幕、番剧课程、Hi-Res/HDR、托盘、设置页、ffmpeg sidecar 打包出安装包，并接入 updater 形成完整发布链路 | 1~1.5 周 |

单人业余节奏约 6~8 周达到 BDL 0.6 的功能覆盖度；M1 结束即有可用产物。

### 2.6 主要风险与对策

| 风险 | 对策 |
|---|---|
| B 站接口/wbi 算法变动 | 接口文档社区维护活跃（socialsisteryi/bilibili-api-collect）；把 API 层做成独立模块便于热修 |
| 风控/429 | 全局限速、请求间隔抖动、批量下载串行化；不做也不需要绕过任何验证（扫码登录是官方通道） |
| 高清清晰度依赖账号 | 明确提示用户；qn 自动降级保证总能下载 |
| ffmpeg 合成音画不同步 | DASH 时间戳问题用 `-muxdelay 0` / 改用 mp4box 兜底 |
| 法律合规 | 仅个人学习用途、不提供付费内容破解、遵守 B 站用户协议，界面注明；打包的 ffmpeg 采用 LGPL 构建并附许可声明与源码链接 |
| GitHub 国内访问不稳导致更新失败 | updater 配多端点 fallback（ghproxy 镜像/自建服务器）；发布同步 Gitee 镜像仓；不做强制更新 |

### 2.7 参考项目（避坑资源）

- **BBDown**（C#，CLI）— playurl/dash 处理逻辑参考
- **DownKyi**（C#，WPF GUI）— 功能清单与交互参考
- **socialsisteryi/bilibili-api-collect** — B 站接口中文文档（wbi 签名、登录、收藏夹等全覆盖，最核心的参考资料）
- **danmaku2ass** — 弹幕转 ass 算法
- 本机 BDL 本体 — 可直接对照其行为验证接口调用是否正确
