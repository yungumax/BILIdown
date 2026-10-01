# BILIdown

B 站视频下载器。技术栈 Tauri 2 + Rust + Vue 3，NSIS 安装包分发，带签名自动更新。

> 视觉与设计规范见 [DESIGN.md](./DESIGN.md)。

## 功能

| 模块 | 能力 |
|---|---|
| **解析** | 批量粘贴或单个输入；视频/合集/收藏夹/系列/番剧/课程/图文/音频全支持；单视频自动展开所在合集 |
| **选择** | 表格逐条勾选，画质（8K~360P）与音轨按优先顺序自动匹配；分组折叠、翻页、已选计数 |
| **下载** | DASH 分片并发、三段式进度（视频流/音频流/合成）、限速、断点续传、启动恢复排队任务 |
| **命名** | 模板变量 19 个（标题/UP/日期/BV 号/序号等）、我的预设、重名自动编号、文件名预览与落盘一致 |
| **内容库** | 登录后浏览收藏夹与订阅合集，与解析页同一套选择/下载流 |
| **传输** | 任务队列与实时状态、速度/进度、取消、清除已结束、打开文件/定位 |
| **设置** | 下载/媒体/命名/文件夹/编码/更新/网络七分类；FFmpeg 自动发现或手动指定；自动更新 |
| **登录** | 扫码登录（凭据仅存本机 `cookies.json`），支持大会员清晰度 |
| **旁挂文件** | 封面 / 弹幕 XML / 字幕，与视频同名独立保存，均可在设置开关 |
| **主题** | 浅色 / 深色 / 跟随系统，原生窗口底色与 CSS 首帧同步 |
| **缩放** | 窗口拉大缩小 UI 等比缩放（`html.zoom`，默认 1100×740 = 1.0） |
| **侧栏** | 宽栏（文字）↔ 图标栏（64px）可切换；速度数字补间滚动 |

## 界面动效（八层）

| 层 | 覆盖 |
|---|---|
| 一 | 切页编排（新页卡片子元素依次浮起）、集合卡逐张浮起、传输新行滑入、速度数字滚动、主按钮磁吸 |
| 二 | 设置分类切换字段行级联、解析结果条目逐个亮起、选择页表格行级联、翻页封面级联、集合卡 3D 微倾斜、计数补间 |
| 三 | Toast 微过冲曲线、勾选弹簧（6% 过冲）、空状态编排、步骤条激活上浮、图标微移、已选计数补间、完成脉冲 |
| 四 | 菜单条目级联（anime.js 驱动）、登录框编排、下拉箭头旋转、页码激活脉冲、头像入场 |
| 五 | 步骤点完成弹跳 + 当前步呼吸、封面 hover 缩放、勾选行序号轻弹、忙碌点脉冲、侧栏宽度平滑变形 |
| 六 | 来源徽标弹跳、登录成功提示弹出、勾选角标角度戏、tab 悬停微升、数字等宽 |
| 七 | 步骤点动效恢复、表格行左缘指示线、解析扫光、空态浮动 |
| 八 | 启动编排（顶栏 + 侧栏导航）、分组展开级联、主题钮旋转、保存脉冲、阶段闪光、传输行指示线 |

全部遵守三条底线：只动 transform/opacity/color、`prefers-reduced-motion` 下全部关闭（颜色反馈保留）、追加式不碰排版。

## 安装与更新

- **新用户**：从 [Releases](https://github.com/yungumax/BILIdown/releases/latest) 下载 `BILIdown_x.y.z_x64-setup.exe`，被动模式安装（不需要管理员权限）。
- **老用户**：应用内「设置 → 应用更新 → 检测更新」一键在线升级（下载签名安装包 → 安装 → 重启）。
- **卸载**：Windows 设置 → 应用 → BILIdown，或运行 `%LOCALAPPDATA%\BILIdown\uninstall.exe`。卸载只删程序本体（`%LOCALAPPDATA%\BILIdown\`），**用户数据与下载文件不在安装目录、不会被删**。
- **数据位置**：登录凭据 `cookies.json` 默认在 `%APPDATA%\com.yungumax.bilidown\`（设置里可改数据目录）；下载文件在设置里指定的输出目录。安装/卸载不影响这些数据。

## 构建

```bash
# 前置：Node.js 22+ / Rust stable / VS Build Tools C++ 工作负载
npm install
npm run build                                     # 前端 → dist/
cargo build --release -p bilidown                 # Rust exe（嵌入 dist/）

# 本地开发
npm run dev                                       # Vite dev server
cargo tauri dev                                   # 或直接跑 src-tauri
```

> MSVC 链接器的 `LIB`/`INCLUDE` 路径如遇异常，参见项目内 `DESIGN.md` 或 CI 工作流的标准配置。

## 测试

```bash
cargo test --workspace          # 93 项单元测试
cargo clippy --workspace --all-targets -- -D warnings   # 零警告
cargo fmt --all --check

# UI 仪器（需要带调试端口启动 exe）
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" ./target/release/bilidown.exe
node tools/test-library.mjs     # 45 台仪器见 tools/README.md
```

## 目录结构

```
src/                        前端（Vue 3）
├── App.vue                 外壳：标题栏 + 侧栏 + 页面切换 + Toast + 启动编排
├── api.js                  与后端通信；浏览器中自动切换为假数据（含魔法变量副本）
├── download-request.js     下载请求与命名变量计算（解析页与内容库共用）
├── styles.css              令牌 + 主题 + 动效层 + 弹层玻璃规则
├── pages/
│   ├── ParsePage.vue       解析 / 选择内容（表格 + 分组 + 工具条 + 下载设置弹层）
│   ├── TransferPage.vue    传输（任务列表 + 统计）
│   ├── LibraryPage.vue     内容库（收藏夹/订阅合集 + 详情封面网格 + 页码）
│   ├── SettingsPage.vue    设置（七分类 + FFmpeg 块 + 应用更新）
│   └── AboutPage.vue       关于
├── components/
│   ├── TitleBar.vue        自定义标题栏（主题菜单 + 登录账号 chip）
│   ├── Sidebar.vue         侧栏（宽/图标两态 + 队列状态）
│   ├── LoginDialog.vue     扫码登录
│   ├── StepHeader.vue      三步指示条
│   ├── TaskRow.vue         传输任务行（三段进度 + 阶段点 + 完成脉冲）
│   └── Icon.vue            iconfont 渲染器
src-tauri/                  后端（Rust / Tauri 2）
├── src/
│   ├── commands.rs         全部 Tauri 命令（解析/下载/设置/更新/维护）
│   ├── state.rs            AppState + Settings + 任务队列
│   ├── naming.rs           命名模板渲染（VARIABLES 真值）
│   └── types.rs            IPC 数据结构
├── tauri.conf.json         应用配置（版本/打包/更新器公钥与端点）
├── capabilities/           权限声明
└── Cargo.toml
crates/bili-core/           独立核心库（API 客户端/解析器/下载器/wbi 签名/登录）
tools/                      45 台 UI 仪器 + 主题/图标生成脚本
.github/workflows/          CI（fmt/clippy/test）+ Release（tauri-action 签名打包）
```

## 已实测

- 扫码登录后 480P → 1080P60 / 杜比视界（HEVC Main10 + hvc1）
- 100+ 条合集批量解析 → 全选 → 逐条下载 → 命名模板含序号补零
- 自动更新端到端：0.1.99 → 0.2.0 → 0.2.1（检测 → 下载进度 → 安装 → 重启）
- NSIS 卸载器：静默卸载后安装目录与注册表项完全清除；重装后版本正确
- 93 项单元测试 / clippy 零警告 / 45 台 UI 仪器全绿
