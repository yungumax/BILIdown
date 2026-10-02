# 项目规则（维护者约定）

## 发版纪律

- **改动完成后默认启动测试版**：构建通过后启动 `target/release/bilidown.exe`（沙箱数据目录，如 `BILIDOWN_COOKIE_FILE=D:\Zcode\_data\bilidown-sandbox\cookies.json`）供验收，**不要**默认拉起装机版。
- **正式发版必须等维护者确认**：确认后再 bump 三处版本号 + `git tag vX.Y.Z` 推送（触发 CI 构建签名安装包并推送自动更新）。装机版（`E:\02GJ\BILIdown`）只在这条流程里升级。
- 日常 commit / push main 到 GitHub 不受此限。

## 构建

改前端后必须两步都跑（exe 编译时嵌入 dist/）：`npm run build` → `cargo build --release --features custom-protocol -p bilidown`。

## 验证

改 UI 后用 `tools/` 下的仪器连真实 exe 验证（CDP 调试端口），回归见 `tools/README.md`。
