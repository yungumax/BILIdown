//! BILIdown 核心库。
//!
//! 分层：`wbi`（签名）→ `client`（HTTP 会话/请求头伪装）→ `api`（业务接口）
//! → `download`（DASH 分片下载）→ `ffmpeg`（合成）。
//! 所有耗时操作通过回调上报进度，便于 CLI 与后续 Tauri 前端复用同一套实现。

pub mod api;
pub mod client;
pub mod danmaku;
pub mod download;
pub mod error;
pub mod ffmpeg;
pub mod login;
pub mod opus;
pub mod parser;
pub mod subtitle;
pub mod util;
pub mod wbi;

pub use api::{DashStream, MediaStream, PlayUrlData, VideoInfo};
pub use client::BiliClient;
pub use download::{DownloadOptions, Progress};
pub use error::{BiliError, Result};
pub use login::{Cookies, LoginState, QrCodeInfo};
pub use util::sanitize_filename;
