# BILIdown

B 站视频下载器。技术栈：Tauri 2 + Rust + Vue 3 + SQLite，ffmpeg 以 sidecar 方式随程序打包，具备 Git 发布链路与自动更新能力。

完整实施方案见 [BDL分析与BILIdown实施方案.md](./BDL分析与BILIdown实施方案.md)。

## 当前进度

| 阶段 | 状态 | 内容 |
|---|---|---|
| M0 工程起步 | ✅ 完成 | 仓库、.gitignore、CI 检查流水线 |
| M1 CLI 原型 | ✅ 完成 | wbi 签名、API 客户端、DASH 分片并发下载、ffmpeg 合成，端到端跑通 |
| M2 GUI 骨架 | ⬜ 待开始 | Tauri 壳 + 链接解析预览 + 任务列表 |
| M3 完整下载 | ⬜ 待开始 | 断点续传、批量、扫码登录 |
| M4 体验完善 | ⬜ 待开始 | 弹幕字幕、番剧课程、ffmpeg 打包、自动更新 |

M1 已验证可用：输入 BV 号即可下载并合成出标准 MP4（h264 + aac），
wbi 签名实现与官方文档示例值逐字一致，匿名访问自动降级到 480P。

## 目录结构

```
crates/
├── bili-core/          核心库（后续 Tauri 后端直接复用）
│   ├── wbi.rs          wbi 签名（含官方示例向量回归测试）
│   ├── client.rs       HTTP 会话、请求头伪装、Cookie、wbi 密钥缓存
│   ├── api.rs          接口与数据结构、清晰度/音轨选择
│   ├── download.rs     DASH 分片并发下载、断点区间写入、进度回调
│   ├── ffmpeg.rs       sidecar 查找与无损合成
│   └── parser.rs       输入解析（BV/av/链接/短链）
└── bili-cli/           命令行原型
```

## 开发环境

- Rust 1.98+（Windows 需 MSVC 生成工具）
- ffmpeg（正式版将内置 sidecar，开发期用系统 PATH 即可）

## 使用

```bash
# 下载视频（默认请求 1080P，未登录会自动降级）
cargo run -p bili-cli -- BV1Vkag6TExf -o ./downloads

# 指定清晰度与并发
cargo run -p bili-cli -- BV1Vkag6TExf -q 80 --concurrency 8

# 带登录态（可获取 1080P 及以上）
cargo run -p bili-cli -- BV1Vkag6TExf --sessdata "你的SESSDATA"

# 查看帮助
cargo run -p bili-cli -- --help
```

## 开发命令

```bash
cargo test --workspace                          # 单元测试
cargo clippy --workspace --all-targets -- -D warnings   # 静态检查
cargo fmt --all                                 # 格式化
```

## 已知限制（M1 范围）

- 仅支持普通投稿视频（BV/av），番剧、课程、收藏夹、合集待 M3
- 仅处理多 P 视频的 P1
- 未登录最高 480P；登录后可解锁 1080P+、4K、HDR、杜比、Hi-Res
- 尚未支持断点续传（已按区间写入，续传只需补记已下载区间）
