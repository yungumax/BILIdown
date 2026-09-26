@echo off
chcp 65001 >nul
cd /d "%~dp0"

set "BIN=target\release\bili-cli.exe"
if not exist "%BIN%" set "BIN=target\debug\bili-cli.exe"

if not exist "%BIN%" (
    echo [ERROR] bili-cli.exe not found.
    echo Build it first:  cargo build -p bili-cli
    echo.
    pause
    exit /b 1
)

"%BIN%" --login %*
echo.
echo Press any key to close this window . . .
pause >nul
