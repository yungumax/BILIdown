# BILIdown

B 站视频下载器。技术栈：Tauri 2 + Rust + Vue 3 + SQLite，ffmpeg 以 sidecar 方式随程序打包，具备 Git 发布链路与自动更新能力。

完整实施方案见 [BDL分析与BILIdown实施方案.md](./BDL分析与BILIdown实施方案.md)。

## 当前进度

| 阶段 | 状态 | 内容 |
|---|---|---|
| M0 工程起步 | ✅ 完成 | 仓库、.gitignore、CI 检查流水线 |
| M1 CLI 原型 | ✅ 完成 | wbi 签名、API 客户端、DASH 分片并发下载、ffmpeg 合成，端到端跑通 |
| M2 桌面界面 | ✅ 完成 | Tauri 2 + Vue 3，界面按 BDL 的视觉语言重做（浅色、左侧导航、步骤条、卡片） |
| M3 批量下载 | ✅ 大部分完成 | 收藏夹/合集/UP 空间/番剧/课程批量、命名模板、重名处理、限速、任务日志 |
| M3 余项 | ⬜ 待开始 | 断点续传与启动恢复、弹幕字幕下载、多 P 视频 |

界面结构：解析（批量/单个解析 → 逐条选择清晰度与音轨 → 全部加入下载）、
传输（任务队列与三段式进度：视频流 / 音频流 / 合成）、内容库（M3 占位）、
设置（保存位置、账号、登录凭据、ffmpeg 状态）、关于。

已实测可用：扫码登录后同一视频从 480P 提升到 1080P60，另成功下载杜比视界（HEVC
Main10 + hvc1）内容；35 个单元测试全绿，clippy 零警告。

## 目录结构

```
src/                    桌面端前端（Vue 3）
├── App.vue             外壳：标题栏 + 侧栏 + 页面切换 + 全局状态
├── api.js              与后端通信；浏览器中打开时自动切换为假数据
├── pages/              解析 / 传输 / 设置 / 关于 / 内容库
└── components/         标题栏（含窗口控制）、侧栏、步骤条、任务行、登录弹窗

src-tauri/              桌面端后端（Tauri 2）
└── src/
    ├── commands.rs     暴露给前端的命令
    ├── state.rs        登录会话、任务表、输出目录
    └── types.rs        IPC 数据结构

crates/
├── bili-core/          核心库（CLI 与桌面端共用）
│   ├── wbi.rs          wbi 签名（含官方示例向量回归测试）
│   ├── client.rs       HTTP 会话、请求头伪装、Cookie、wbi 密钥缓存
│   ├── api.rs          接口与数据结构、清晰度/音轨选择
│   ├── download.rs     DASH 分片并发下载、断点区间写入、进度回调
│   ├── ffmpeg.rs       sidecar 查找与无损合成
│   ├── login.rs        扫码登录
│   ├── parser.rs       输入解析（BV/av/链接/短链）
│   └── util.rs         文件名清洗等
└── bili-cli/           命令行
```

## 快捷脚本

| 脚本 | 用途 |
|---|---|
| `dev.bat` | 开发模式（热更新）；`dev.bat build` 打包安装程序 |
| `login.bat` | 命令行扫码登录（终端显示二维码） |

## 开发环境

- Rust 1.98+（Windows 需 MSVC 生成工具）
- Node.js 20+（前端构建）
- ffmpeg（正式版将内置 sidecar，开发期用系统 PATH 即可）

## 运行桌面端

**Windows 下推荐双击 `dev.bat`**（会自动带上 Node 路径）：

- 直接双击 = 开发模式：起前端热更新服务并打开窗口
- `dev.bat build` = 打包出安装程序

等效命令（需要 Node 在 PATH 里）：

```bash
npm install            # 首次
npm run tauri dev      # 开发模式
npm run tauri build    # 打包安装程序（M4 会加上 ffmpeg 与自动更新）
```

> **注意**：`cargo run -p bilidown` 出来的 debug 版本会去连开发服务器
> （http://localhost:5173）。开发服务器没起时窗口里会显示「localhost 拒绝连接」——
> 这是 Tauri 的预期行为，不是程序坏了。
>
> 要一个不依赖开发服务器的可执行文件，**必须启用 `custom-protocol` feature**
> （Tauri 靠它区分开发/生产：没开就按开发模式连 devUrl）：
>
> ```bash
> cargo build --release -p bilidown --features custom-protocol
> ```
>
> 用 `npm run tauri build`（或 `dev.bat build`）打包时会自动启用，无需手动指定。

只调界面时可以直接 `npm run dev` 打开 http://localhost:5173 —— 检测不到 Tauri
运行时会自动使用假数据，改样式不必反复编译 Rust。

## 命令行使用

```bash
# 下载视频（默认请求 1080P，未登录会自动降级）
cargo run -p bili-cli -- BV1Vkag6TExf -o ./downloads

# 指定清晰度与并发
cargo run -p bili-cli -- BV1Vkag6TExf -q 80 --concurrency 8

# 查看帮助
cargo run -p bili-cli -- --help
```

### 扫码登录（解锁 1080P 及以上）

**Windows 下最省事的方式**：双击项目根目录的 **`login.bat`**，会弹出终端窗口并显示二维码，
用 B 站手机客户端「我的 → 扫一扫」扫描，手机上确认后窗口会显示登录结果，按任意键关闭即可。

命令行方式（等效）：

```bash
# 扫码登录
cargo run -p bili-cli -- --login

# 退出登录（删除已保存的登录态）
cargo run -p bili-cli -- --logout

# 登录后直接下载即可自动使用登录态
cargo run -p bili-cli -- BV1Vkag6TExf -q 120      # 120 = 4K
```

登录态默认保存在 `D:\Zcode\_data\bilidown\cookies.json`（可用 `--cookie-file` 或环境变量
`BILIDOWN_COOKIE_FILE` 改路径）。该文件等同于账号凭据，请勿分享或提交到仓库。

也可以退化为手动模式：`--sessdata "你的SESSDATA"`（优先级高于登录态文件）。

### 清晰度

| 清晰度 | qn | 要求 |
|---|---|---|
| 360P / 480P | 16 / 32 | 无需登录 |
| 720P | 64 | 登录 |
| 1080P / 1080P60 / 1080P+ | 80 / 116 / 112 | 登录 |
| 4K / HDR / 杜比视界 | 120 / 125 / 126 | 大会员 |
| Hi-Res 无损音轨 | `--audio flac` | 大会员 |

请求的清晰度不可得时会自动降级到该视频可用的最高档，并说明是「视频本身没有」还是
「账号权限不足」。HEVC 内容会打上 `hvc1` 标签，保证播放器兼容性。

## 开发命令

```bash
cargo test --workspace                          # 单元测试
cargo clippy --workspace --all-targets -- -D warnings   # 静态检查
cargo fmt --all                                 # 格式化

# 需要联网的手动测试（默认为跳过）
cargo test -p bili-core -- --ignored --nocapture
```

## 已知限制

- 多 P 视频仅下载 P1
- 尚未支持断点续传（已按区间写入，续传只需补记已下载区间）
- 弹幕与字幕下载待后续版本（嵌入字幕选项已预留）
- 收藏夹/合集默认拉取上限 500 条、UP 空间 300 条；超过上限的来源在选择内容页用「解析 → 加载下一批 / 按序号加载…」分批拉完（两批互不重叠，配合默认的「重名跳过」不会重复下载）。上限可在设置里改，0 表示用默认值，最大 20000 条。
