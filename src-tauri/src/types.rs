//! 前后端 IPC 的数据结构（字段名与前端 TypeScript/JS 侧保持一致）。

use serde::{Deserialize, Serialize};

/// 登录状态，用于顶栏展示。
#[derive(Debug, Clone, Default, Serialize)]
pub struct LoginInfo {
    pub logged_in: bool,
    pub uname: String,
    pub mid: u64,
    pub vip: bool,
    pub vip_label: String,
}

/// 应用启动时的整体状态。
#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub version: String,
    pub login: LoginInfo,
    pub output_dir: String,
    pub cookies_path: String,
}

/// 可选清晰度。
#[derive(Debug, Clone, Serialize)]
pub struct QualityOption {
    pub qn: u32,
    pub label: String,
    /// 当前账号是否真的能拿到这一档
    pub available: bool,
    /// 拿不到时的原因提示，例如「需登录」「需大会员」
    pub hint: String,
}

/// 可选音轨。
#[derive(Debug, Clone, Serialize)]
pub struct AudioOption {
    /// normal / dolby / flac
    pub kind: String,
    pub label: String,
    pub available: bool,
}

/// 解析结果，驱动预览卡片。
#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub bvid: String,
    pub cid: u64,
    pub title: String,
    pub owner: String,
    /// 秒
    pub duration: u64,
    /// 封面 data URL（已按缩略图尺寸请求），失败时为空串
    pub cover: String,
    pub page_count: usize,
    /// 需要提示给用户的说明，例如分 P 只处理 P1
    pub note: String,
    pub qualities: Vec<QualityOption>,
    pub audios: Vec<AudioOption>,
    pub recommended_quality: u32,
    pub best_quality: u32,
}

/// 前端发起的下载请求。
#[derive(Debug, Clone, Deserialize)]
pub struct DownloadRequest {
    pub bvid: String,
    pub cid: u64,
    pub title: String,
    pub quality: u32,
    /// normal / dolby / flac
    pub audio: String,
    /// 封面 data URL，仅用于任务行展示
    #[serde(default)]
    pub cover: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    /// 已入队，等待下载槽位
    Queued,
    Downloading,
    Merging,
    Done,
    Failed,
    Canceled,
}

/// 任务快照，通过 `task://update` 事件推给前端。
#[derive(Debug, Clone, Serialize)]
pub struct TaskUpdate {
    pub id: String,
    pub title: String,
    pub quality_label: String,
    pub status: TaskStatus,
    pub video_pct: f64,
    pub audio_pct: f64,
    pub downloaded: u64,
    pub total: u64,
    pub speed_bps: f64,
    pub output_path: String,
    pub message: String,
    pub cover: String,
    #[serde(skip)]
    pub video_bytes: u64,
    #[serde(skip)]
    pub video_total: u64,
    #[serde(skip)]
    pub audio_bytes: u64,
    #[serde(skip)]
    pub audio_total: u64,
    #[serde(skip)]
    pub bvid: String,
}

impl TaskUpdate {
    pub fn new(id: String, req: &DownloadRequest) -> Self {
        Self {
            id,
            title: req.title.clone(),
            quality_label: String::new(),
            status: TaskStatus::Queued,
            video_pct: 0.0,
            audio_pct: 0.0,
            downloaded: 0,
            total: 0,
            speed_bps: 0.0,
            output_path: String::new(),
            message: "排队中".to_string(),
            cover: req.cover.clone(),
            video_bytes: 0,
            video_total: 0,
            audio_bytes: 0,
            audio_total: 0,
            bvid: req.bvid.clone(),
        }
    }

    /// 由各分轨的字节数汇总出总进度。
    pub fn recalc(&mut self) {
        self.downloaded = self.video_bytes + self.audio_bytes;
        self.total = self.video_total + self.audio_total;
    }
}

/// 扫码登录：二维码信息。
#[derive(Debug, Clone, Serialize)]
pub struct QrInfo {
    pub url: String,
    pub qrcode_key: String,
}

/// 扫码登录：轮询结果。
#[derive(Debug, Clone, Serialize)]
pub struct LoginPoll {
    /// pending / scanned / confirmed / expired
    pub state: String,
    pub login: LoginInfo,
}

/// 设置页需要的运行环境信息。
#[derive(Debug, Clone, Serialize)]
pub struct AppSettings {
    pub output_dir: String,
    pub cookies_path: String,
    pub cookies_saved: bool,
    pub ffmpeg_ok: bool,
    pub ffmpeg_info: String,
    pub version: String,
}
