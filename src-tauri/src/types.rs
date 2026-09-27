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

/// 批量来源里的一条可下载视频（番剧/课程条目可能没有 bvid，用 ep_id 标识）。
#[derive(Debug, Clone, Serialize)]
pub struct BatchVideo {
    pub bvid: String,
    pub cid: u64,
    pub ep_id: Option<u64>,
    /// 图文（opus）条目的 id；视频条目为空。图文的"内容"是图片与正文，没有视频流。
    #[serde(default)]
    pub opus_id: String,
    /// 音频（au）条目的 id；其余来源为空。音频下载的是音频流而不是视频。
    #[serde(default)]
    pub au_id: String,
    /// 这条内容所属的合集名（UP 投稿来源里逐条判断；不在任何合集时为空）
    #[serde(default)]
    pub collection: String,
    pub title: String,
    /// 该条目的 UP 主 / 出品方；来源没给就是空
    #[serde(default)]
    pub owner: String,
    /// 秒
    pub duration: u64,
}

/// 统一的解析结果：
/// - kind=video：单视频，bvid/cid/cover/duration 有效，items 为空
/// - kind=fav/collection/space：普通视频批量
/// - kind=bangumi/cheese：剧集批量
#[derive(Debug, Clone, Serialize)]
pub struct ProbeSource {
    pub kind: String,
    /// 来源身份（如 collection:100:200、video:BV1xx）：前端用它判重，
    /// 同一个合集的多个视频、同一链接贴两次都会被识别成同一个来源
    #[serde(default)]
    pub key: String,
    pub title: String,
    pub owner: String,
    /// 封面 data URL（仅单视频）
    pub cover: String,
    pub note: String,
    /// 单视频：BV 号
    pub bvid: String,
    pub cid: u64,
    /// 单视频：AV 号 / UP 主 mid / 发布时间（Unix 秒）；批量来源为 0
    pub aid: u64,
    pub owner_mid: u64,
    pub pubdate: u64,
    /// 单视频首页分 P 的序号与标题（单 P 时即 P1），用于 {part_index} {part_title}
    pub part_index: u32,
    pub part_title: String,
    /// 单视频时长（秒）
    pub duration: u64,
    /// 多 P 视频的分 P 数
    pub page_count: usize,
    /// 源内总条数（可能因分页上限被截断）
    pub total: usize,
    /// 这批第一条在来源里的序号（1 起）。按序号加载时不是 1；
    /// 单视频与整批加载都是 1。
    #[serde(default)]
    pub from_index: usize,
    /// 实际加载条数
    pub loaded: usize,
    /// 是否已经拉到底（「继续解析」没有更多了）
    pub exhausted: bool,
    /// 停止是因为撞到单次上限（而不是来源取完了）——只有这种才该给"加载下一批"
    #[serde(default)]
    pub capped: bool,
    pub qualities: Vec<QualityOption>,
    pub audios: Vec<AudioOption>,
    pub recommended_quality: u32,
    pub best_quality: u32,
    pub items: Vec<BatchVideo>,
}

/// 前端发起的下载请求。
#[derive(Debug, Clone, Deserialize)]
pub struct DownloadRequest {
    pub bvid: String,
    pub cid: u64,
    pub title: String,
    /// 来源：video（默认）/ bangumi / cheese
    #[serde(default)]
    pub source: String,
    /// 番剧 ep_id / 课程 ep_id
    #[serde(default)]
    pub ep_id: Option<u64>,
    /// 图文（opus）id；source=opus 时用它取内容
    #[serde(default)]
    pub opus_id: String,
    /// 音频（au）id；source=audio 时用它取播放地址
    #[serde(default)]
    pub au_id: String,
    /// UP 主名
    #[serde(default)]
    pub owner: String,
    pub quality: u32,
    /// normal / dolby / flac
    pub audio: String,
    /// 封面 data URL，仅用于任务行展示
    #[serde(default)]
    pub cover: String,
    /// 命名模板的其余变量取值，前端从解析结果带过来
    #[serde(default)]
    pub naming: NamingMeta,
}

/// 命名模板里前端能提供的变量取值。取不到的留空——空值在文件名里直接消失。
/// 清晰度、编码、扩展名由后端在任务执行时补齐。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct NamingMeta {
    /// 来源类型中文名（文件夹模板里的 {source_kind}）
    #[serde(default)]
    pub source_kind: String,
    /// {index} 的补零宽度（0 = 不补；前端按本批条数给，几千条就是 4）
    #[serde(default)]
    pub index_pad: u32,
    /// 本地时区相对 UTC 的偏移（分钟，东为正）。图文按发布日期编号时要用它换算本地日期
    #[serde(default)]
    pub tz_offset_min: i32,
    pub part_title: String,
    pub part_index: u32,
    pub aid: u64,
    pub owner_mid: u64,
    pub series_title: String,
    pub episode_index: u32,
    pub episode_title: String,
    pub collection_title: String,
    pub index: u32,
    /// 已经按本地时间格式化好的 YYYY-MM-DD
    pub date: String,
    pub publish_date: String,
}

/// 「魔法变量」面板的一项。
#[derive(Debug, Clone, Serialize)]
pub struct NamingVariable {
    /// 栏目（面板里一列一个，横排）
    #[serde(default)]
    pub section: String,
    pub token: String,
    /// 列里显示的短标签
    pub label: String,
    /// 悬停说明；空表示短标签已经说清
    #[serde(default)]
    pub hint: String,
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
    #[serde(skip)]
    pub ep_id: Option<u64>,
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
            ep_id: req.ep_id,
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

/// 设置页数据：可编辑项 + 只读的运行环境信息。
#[derive(Debug, Clone, Serialize)]
pub struct AppSettings {
    pub settings: crate::state::Settings,
    pub cookies_path: String,
    pub cookies_saved: bool,
    pub ffmpeg_ok: bool,
    pub ffmpeg_info: String,
    pub version: String,
}

/// 「继续解析」的返回：本次新增的条目 + 最新进度。
#[derive(Debug, Clone, Serialize)]
pub struct ProbeMore {
    pub items: Vec<BatchVideo>,
    pub loaded: usize,
    pub total: usize,
    pub exhausted: bool,
    /// 见 [`ProbeSource::capped`]：续拉之后也要带着，否则界面会丢掉"加载下一批"
    #[serde(default)]
    pub capped: bool,
    pub note: String,
}

/// 文件名预览的入参：一条内容 + 它的命名变量取值。
/// 变量取值由前端按来源类型算好（与真正下载时提交的是同一套），
/// 这里只负责用同一个渲染器算出文件名，保证预览和落盘一致。
#[derive(Debug, Clone, Deserialize)]
pub struct NamingPreviewItem {
    pub title: String,
    /// 条目的 UP 主名（文件夹层级模板里的 {owner_name} 用它）
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub bvid: String,
    #[serde(default)]
    pub cid: u64,
    /// 来源类型：音频固定 m4a，图文/专栏是文件夹（不给扩展名），其余按封装设置
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub naming: NamingMeta,
}
