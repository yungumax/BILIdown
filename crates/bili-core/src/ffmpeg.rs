//! ffmpeg 调用：音视频合成。
//!
//! 查找顺序：显式指定路径 → 程序同目录的 sidecar（打包形态）→ PATH。

use crate::error::{BiliError, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Tauri 打包时 sidecar 会按目标三元组重命名。
const SIDECAR_NAMES: [&str; 3] = ["ffmpeg-x86_64-pc-windows-msvc.exe", "ffmpeg.exe", "ffmpeg"];

/// Windows：不给子进程分配控制台。
///
/// 本程序是 GUI 子系统，启动控制台程序（ffmpeg）时系统默认会新建一个终端窗口：
/// 只是探测版本也会弹出一个终端一闪而过，合成时终端更会一直挂着。
/// 用窗口事件钩子实测，每次 spawn 都会 SHOW/HIDE 一个 Windows Terminal 窗口。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
fn quiet(command: &mut Command) -> &mut Command {
    command.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
fn quiet(command: &mut Command) -> &mut Command {
    command
}

pub fn find_ffmpeg(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        if path.exists() {
            return Some(path.to_path_buf());
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in SIDECAR_NAMES {
                let candidate = dir.join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    path_ffmpeg()
}

/// 在 PATH 目录里找 ffmpeg。
///
/// 刻意不靠执行 `ffmpeg -version` 来判断可用性：那会多起一次子进程
/// （Windows 上还会多弹一次终端窗口），版本探测另有 `probe_version`。
fn path_ffmpeg() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in ["ffmpeg.exe", "ffmpeg"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 返回版本首行，用于启动时自检。
pub async fn probe_version(ffmpeg: &Path) -> Result<String> {
    let mut command = Command::new(ffmpeg);
    let output = quiet(&mut command)
        .arg("-version")
        .output()
        .await
        .map_err(|e| BiliError::FfmpegUnavailable(format!("{}: {e}", ffmpeg.display())))?;

    if !output.status.success() {
        return Err(BiliError::FfmpegUnavailable(format!(
            "{} 执行返回失败",
            ffmpeg.display()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string())
}

/// 封装格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Mp4,
    /// MPEG-TS：原样流封装，剪辑/直播工具友好，不做 faststart 重排
    Ts,
}

impl Container {
    /// 从设置值解析；未知值回退 MP4（老设置里的 mkv 也走这里 —— 封面字幕弹幕
    /// 都改成独立文件后 MKV 没用了，已从界面去掉）。
    pub fn parse(value: &str) -> Self {
        if value == "ts" {
            Self::Ts
        } else {
            Self::Mp4
        }
    }
}

/// 把音视频合成到 `out`，`-c copy` 直接封装不做重编码，通常几秒内完成。
///
/// - MP4：HEVC 写成 `hvc1` 标签（默认 `hev1` 多数播放器不识别），加 faststart
/// - TS：什么都不加，让 ffmpeg 按 `.ts` 选 mpegts 封装
pub async fn merge_video_audio(
    ffmpeg: &Path,
    video: &Path,
    audio: &Path,
    out: &Path,
    container: Container,
    hevc: bool,
) -> Result<()> {
    let mut command = Command::new(ffmpeg);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(video)
        .arg("-i")
        .arg(audio)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("1:a:0")
        .arg("-c")
        .arg("copy");

    // 封面不再嵌进容器（改成旁边放一份独立图片），所以 MKV 这边没有额外参数
    if container == Container::Mp4 {
        if hevc {
            command.arg("-tag:v").arg("hvc1");
        }
        command.arg("-movflags").arg("+faststart");
    }

    let output = quiet(&mut command)
        .arg(out)
        .output()
        .await
        .map_err(|e| BiliError::FfmpegUnavailable(format!("{}: {e}", ffmpeg.display())))?;

    if !output.status.success() {
        return Err(BiliError::FfmpegFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(())
}

/// 转成 MP3（VBR，约 190kbps）。音频来源里选了 MP3 时用；失败由调用方兜底成原格式。
pub async fn to_mp3(ffmpeg: &Path, input: &Path, output: &Path) -> Result<()> {
    let mut command = Command::new(ffmpeg);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-vn")
        .arg("-c:a")
        .arg("libmp3lame")
        .arg("-q:a")
        .arg("2")
        .arg(output);
    run_quiet(&mut command, ffmpeg).await
}

/// 图片转 JPEG（质量 2，接近视觉无损）。
pub async fn to_jpg(ffmpeg: &Path, input: &Path, output: &Path) -> Result<()> {
    let mut command = Command::new(ffmpeg);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("2")
        // 显式指定封装：调用方给的临时名可能不是 .jpg 结尾，靠扩展名猜会直接报错
        .arg("-f")
        .arg("image2")
        .arg(output);
    run_quiet(&mut command, ffmpeg).await
}

/// 跑一条 ffmpeg 命令，失败时把 stderr 带出来。
async fn run_quiet(command: &mut Command, ffmpeg: &Path) -> Result<()> {
    let output = quiet(command)
        .output()
        .await
        .map_err(|e| BiliError::FfmpegUnavailable(format!("{}: {e}", ffmpeg.display())))?;
    if !output.status.success() {
        return Err(BiliError::FfmpegFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_parses_known_values_and_falls_back() {
        assert!(matches!(Container::parse("mp4"), Container::Mp4));
        assert!(matches!(Container::parse("ts"), Container::Ts));
        // 老设置里的 mkv（以及任何乱值）都落回 MP4
        assert!(matches!(Container::parse("mkv"), Container::Mp4));
        assert!(matches!(Container::parse(""), Container::Mp4));
    }
}
