@echo off
chcp 65001 >nul
cd /d "%~dp0"

rem 开发环境自带的 Node 不在系统 PATH 里，这里显式加上
set "NODE_DIR=C:\Users\87845\.workbuddy\binaries\node\versions\22.22.2-3"
if not exist "%NODE_DIR%\npm.cmd" (
    echo [ERROR] 未找到 Node: %NODE_DIR%
    echo 请修改本脚本里的 NODE_DIR 指向实际安装目录。
    pause
    exit /b 1
)

set "PATH=%NODE_DIR%;%PATH%"

if /i "%~1"=="build" (
    echo 正在打包安装程序 ...
    call npm run tauri build
) else (
    echo 正在启动开发模式（前端热更新 + 桌面窗口）...
    call npm run tauri dev
)

echo.
echo 命令已结束。
pause >nul
