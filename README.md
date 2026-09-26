# BILIdown

B 站视频下载器。技术栈：Tauri 2 + Rust + Vue 3 + SQLite，ffmpeg 以 sidecar 方式随程序打包，具备 Git 发布链路与自动更新能力。

完整实施方案见 [BDL分析与BILIdown实施方案.md](./BDL分析与BILIdown实施方案.md)。

## 当前进度

| 阶段 | 状态 | 内容 |
|---|---|---|
| M0 工程起步 | ✅ 完成 | 仓库、.gitignore、CI 检查流水线 |
| M1 CLI 原型 | ✅ 完成 | wbi 签名、API 客户端、DASH 分片并发下载、ffmpeg 合成，端到端跑通 |
| M2 桌面界面 | ✅ 完成 | Tauri 2 + Vue 3 界面：链接解析预览、清晰度/音轨选择、任务列表与三段式进度、扫码登录弹窗、账号状态 |
| M3 完整下载 | ⬜ 待开始 | 断点续传、批量（收藏夹/合集/UP 主）、弹幕字幕 |
| M4 体验完善 | ⬜ 待开始 | 番剧课程、ffmpeg 打包、自动更新 |

已实测可用：扫码登录后同一视频从 480P 提升到 1080P60，另成功下载杜比视界（HEVC
Main10 + hvc1）内容；32 个单元测试全绿，clippy 零警告。

## 目录结构

```
src/                    桌面端前端（Vue 3）
├── App.vue             主界面：链接输入、任务列表、状态栏
├── api.js              与后端通信；浏览器中打开时自动切换为假数据
└── components/         预览卡片、任务行（含三段式进度）、登录弹窗

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

## 开发环境

- Rust 1.98+（Windows 需 MSVC 生成工具）
- Node.js 20+（前端构建）
- ffmpeg（正式版将内置 sidecar，开发期用系统 PATH 即可）

## 运行桌面端

```bash
npm install            # 首次
npm run tauri dev      # 开发模式（自动起前端 dev server 并打开窗口）

npm run tauri build    # 打包出安装程序（M4 会加上 ffmpeg 与自动更新）
```

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

## 已知限制（M1 范围）

- 仅支持普通投稿视频（BV/av），番剧、课程、收藏夹、合集待 M3
- 仅处理多 P 视频的 P1
- 尚未支持断点续传（已按区间写入，续传只需补记已下载区间）
- 弹幕与字幕下载待 M3
