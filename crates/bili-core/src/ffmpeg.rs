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
    Mkv,
}

impl Container {
    /// 从设置值解析；未知值回退 MP4。
    pub fn parse(value: &str) -> Self {
        if value == "mkv" {
            Self::Mkv
        } else {
            Self::Mp4
        }
    }
}

/// 把音视频合成到 `out`，`-c copy` 直接封装不做重编码，通常几秒内完成。
///
/// - MP4：HEVC 写成 `hvc1` 标签（默认 `hev1` 多数播放器不识别），加 faststart
/// - MKV：无 faststart；`cover` 传入封面图时作为附件嵌入（播放器显示为海报）
pub async fn merge_video_audio(
    ffmpeg: &Path,
    video: &Path,
    audio: &Path,
    out: &Path,
    container: Container,
    hevc: bool,
    cover: Option<(&Path, &str)>,
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

    match container {
        Container::Mp4 => {
            if hevc {
                command.arg("-tag:v").arg("hvc1");
            }
            command.arg("-movflags").arg("+faststart");
        }
        Container::Mkv => {
            if let Some((cover_path, mime)) = cover {
                command
                    .arg("-attach")
                    .arg(cover_path)
                    .arg("-metadata:s:t")
                    .arg(format!("mimetype={mime}"));
            }
        }
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
