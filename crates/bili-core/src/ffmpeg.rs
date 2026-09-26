//! ffmpeg 调用：音视频合成。
//!
//! 查找顺序：显式指定路径 → 程序同目录的 sidecar（打包形态）→ PATH。

use crate::error::{BiliError, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Tauri 打包时 sidecar 会按目标三元组重命名。
const SIDECAR_NAMES: [&str; 3] = ["ffmpeg-x86_64-pc-windows-msvc.exe", "ffmpeg.exe", "ffmpeg"];

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

fn path_ffmpeg() -> Option<PathBuf> {
    let output = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .ok()?;
    output.status.success().then(|| PathBuf::from("ffmpeg"))
}

/// 返回版本首行，用于启动时自检。
pub async fn probe_version(ffmpeg: &Path) -> Result<String> {
    let output = Command::new(ffmpeg)
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

/// 用 `-c copy` 直接封装，不做重编码，通常几秒内完成。
pub async fn merge_video_audio(
    ffmpeg: &Path,
    video: &Path,
    audio: &Path,
    out: &Path,
) -> Result<()> {
    let output = Command::new(ffmpeg)
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
        .arg("copy")
        .arg("-movflags")
        .arg("+faststart")
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
