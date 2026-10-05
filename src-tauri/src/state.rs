//! 应用运行期状态：登录会话、任务表、可持久化的设置。

use crate::types::TaskUpdate;
use bili_core::BiliClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::Semaphore;

/// 默认同时下载任务数。
pub const DEFAULT_MAX_CONCURRENT_TASKS: usize = 2;

pub struct TaskEntry {
    pub snapshot: Arc<Mutex<TaskUpdate>>,
    pub abort: Option<tokio::task::AbortHandle>,
    /// 原始请求：暂停后恢复要从头重跑下载（排队中就暂停的任务，磁盘上还没有
    /// task.json，只能靠内存里这份；见 commands::resume_download）
    pub req: Option<std::sync::Arc<crate::types::DownloadRequest>>,
}

pub struct AppState {
    /// 用 `RwLock` 包一层，改代理或登出时可以整体换成新的会话
    client: RwLock<Arc<BiliClient>>,
    pub tasks: Mutex<HashMap<String, TaskEntry>>,
    settings: Mutex<Settings>,
    /// 并发下载槽位，会随设置变化重建
    slots: RwLock<Arc<Semaphore>>,
    counter: Mutex<u64>,
    /// ffmpeg 探测结果缓存。探测要起子进程（实测约 0.8 秒），
    /// 不能放进每次都会调用的设置读取里，否则启动与每次保存都要等它。
    ffmpeg: Mutex<Option<FfmpegStatus>>,
    /// 批量来源的增量加载缓存，按来源输入索引
    batches: Mutex<HashMap<String, BatchCache>>,
    /// 会话里是否已经"热过身"（访问过首页、拿到 buvid3 等风控 Cookie）。
    /// 只做一次：这些 Cookie 是字幕、播放地址这些接口被 412 挡掉的一个常见原因。
    warmed: std::sync::atomic::AtomicBool,
}

/// ffmpeg 可用性：`ok` 表示探测通过，`info` 为版本行或失败原因。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfmpegStatus {
    pub ok: bool,
    pub info: String,
}

/// 批量来源的分页参数：重新拉某一页时需要知道来源本身。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchTarget {
    Fav(u64),
    Collection {
        mid: u64,
        sid: u64,
    },
    /// 系列：链接与合集同形，接口不同（见 parser 里的说明）
    Series {
        mid: u64,
        sid: u64,
    },
    /// 图文列表：mid（游标翻页，见 next_offset）
    Opus(u64),
    /// 音频投稿列表：mid
    Audio(u64),
    Space(u64),
    /// 番剧/课程一次给全，没有分页
    Whole,
}

/// 合集标签：解析 UP 空间时从合集枚举里一次拿到的归属与 cid。
/// cid 一并带全，下载时连 video_info 补查都省掉。
#[derive(Debug, Clone)]
pub struct CollectionTag {
    pub title: String,
    pub cid: u64,
}

/// 批量来源的增量加载缓存。
///
/// 解析时只拉第一页，「继续解析」接着往后拉：收藏夹 130 条要 7 次请求，
/// 每次都从第一页重来会很浪费，也更容易触发风控。
#[derive(Debug, Clone)]
pub struct BatchCache {
    pub kind: String,
    pub target: BatchTarget,
    pub title: String,
    pub owner: String,
    /// 来源声明的总条数
    pub total: usize,
    pub items: Vec<crate::types::BatchVideo>,
    /// 空间来源的合集映射（bvid → 合集名 + cid）。只在首次解析建一次，
    /// 「继续解析」的后续页直接查它，不再重复请求。
    pub collections: HashMap<String, CollectionTag>,
    /// 合集映射是否完整枚举成功：只有 true 才允许前端按合集分组、
    /// 下载时跳过逐条补查（此时"不在任何合集"也会被标成"单独投稿"）。
    pub collections_mapped: bool,
    /// 这批第一条在来源里的序号（1 起）。按序号加载时不是 1，
    /// 界面上的序号列与命名模板的 {index} 都按它换算成来源内的真实位置。
    pub from_index: usize,
    /// 这批的单次上限（创建时的设置快照，改设置不影响已经打开的清单）
    pub cap: usize,
    /// 下次要拉的页码
    pub next_page: u32,
    /// 图文列表的下一页游标（opus 按 offset 翻页，页码参数无效）
    pub next_offset: String,
    /// 已经拉完（没有更多，或到了单次上限）
    pub exhausted: bool,
    pub qualities: Vec<crate::types::QualityOption>,
    pub audios: Vec<crate::types::AudioOption>,
    pub recommended_quality: u32,
    pub best_quality: u32,
}

/// 文件夹层级的默认模板：UP → 合集 →（条目自身）。
/// 不是 UP 来源（例如单个视频直链）时 owner_name 为空，整层自然消失。
pub const DEFAULT_FOLDER_TEMPLATE: &str = "{owner_name}/{collection_title}";

/// 缓存条目上限：同一来源边看边拉时只会有几条，超了丢最早的一条。
const MAX_BATCH_CACHE: usize = 8;

/// 一条画质优先项：目标档位 + 同档内的编码偏好（auto/avc/hevc/av1）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityPref {
    pub qn: u32,
    #[serde(default = "default_codec")]
    pub codec: String,
}

/// 音频/图片格式的默认值：原样，不转码。
fn default_format_source() -> String {
    "source".to_string()
}

fn default_codec() -> String {
    "auto".to_string()
}

/// 一条命名模板预设：用户可把常用模板存成名字，随时选用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NamingPreset {
    pub name: String,
    pub template: String,
}

/// 用户可配置项，全部持久化到 settings.json。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub output_dir: PathBuf,
    /// 文件夹层级模板：用 `/` 分层，为空表示不建层级。
    /// 默认按"UP → 合集"分层（不是 UP 来源时那一层自然消失）。
    pub folder_template: String,
    /// 同时下载的任务数上限
    pub max_concurrent_tasks: usize,
    /// 单个任务的分片并发数
    pub chunk_concurrency: usize,
    /// 分片大小（MB）
    pub chunk_mb: u64,
    /// 下载完成后保留音视频分轨文件
    pub keep_temp: bool,
    /// 旧版命名枚举，保留只为兼容旧 settings.json；新逻辑用 naming_template
    pub naming: String,
    /// 命名模板，可用标记：{title} {bvid} {quality} {owner}
    pub naming_template: String,
    /// 用户保存的命名模板预设（同名覆盖）
    #[serde(default)]
    pub naming_presets: Vec<NamingPreset>,
    /// 用户保存的文件夹模板预设（同名覆盖）。
    /// 模板允许为空——"不建文件夹"本身就是一个正当的预设。
    #[serde(default)]
    pub folder_presets: Vec<NamingPreset>,
    /// 重名处理：skip=跳过任务 / overwrite=覆盖 / auto=自动加序号
    pub rename_conflict: String,
    /// 视频封装：mp4 / ts（MKV 已去掉：封面、字幕、弹幕都成独立文件后它没用了）
    pub container: String,
    /// 音频（音频来源）成品格式：source=原轨道不转码 / mp3=转成 MP3
    #[serde(default = "default_format_source")]
    pub audio_format: String,
    /// 图片（图文图片与封面）格式：source=原格式不转码 / jpg=统一转 JPEG
    #[serde(default = "default_format_source")]
    pub image_format: String,
    /// 视频编码偏好：auto / avc / hevc
    pub codec_pref: String,
    /// 请求的清晰度不可用时：nearest=自动降级 / fail=任务失败
    pub quality_fallback: String,
    /// 下载封面：与视频同名的独立图片文件，不合成进视频
    #[serde(default)]
    pub download_cover: bool,
    /// 下载字幕：与视频同名的独立 .srt。**界面上暂时没有这个开关** ——
    /// 字幕清单接口（x/player/wbi/v2）被 B 站的 gaia 风控挡住（412），实测拿不到数据。
    /// 代码与开关都留着：哪天能够过风控，把媒体页的勾选框加回来即可。
    #[serde(default)]
    pub download_subtitles: bool,
    /// 下载弹幕：单独存一份与视频同名的 .xml，播放器直接读；不与视频合成
    #[serde(default)]
    pub download_danmaku: bool,
    /// 旧字段：曾经是"把封面/字幕嵌进 MKV"。现在一律改成独立文件，
    /// 这两个只用于迁移（勾过就当作要下载），不再写回设置。
    #[serde(default, skip_serializing)]
    pub embed_cover: bool,
    #[serde(default, skip_serializing)]
    pub embed_subtitles: bool,
    /// 单个分片失败的最大重试次数
    pub retry_count: u32,
    /// 全局限速（MiB/s），0 表示不限速
    pub speed_limit_mib: u32,
    /// 播放地址过期时自动刷新（配合断点续传，后续版本生效）
    pub auto_refresh_urls: bool,
    /// 启动时自动继续未完成任务（断点续传在后续版本提供）
    pub resume_on_start: bool,
    /// 解析节奏预设名（仅用于界面回显）
    pub parse_preset: String,
    /// 每批解析条数
    pub parse_batch: u32,
    /// 批间等待（毫秒）
    pub parse_batch_wait_ms: u32,
    /// 每解析多少条休息一次
    pub parse_rest_every: u32,
    /// 休息时长（毫秒）
    pub parse_rest_ms: u32,
    /// 单次解析上限：0 表示按来源类型给默认值（合集/收藏夹 500、UP 空间 300）。
    /// 超过上限的来源用「按序号加载」分几次拉完。
    pub parse_cap: usize,
    /// 自定义 ffmpeg 路径，留空自动发现
    pub ffmpeg_path: String,
    /// 启动时静默检查更新
    pub update_check: bool,
    /// 任务日志级别：debug / info / warn / error
    pub log_level: String,
    /// 数据目录（日志等），留空用默认数据目录
    pub data_dir: String,
    /// 默认清晰度，0 表示自动取可用最高档
    /// （旧字段：新装或迁移后由 `quality_prefs` 承担，仅为读旧配置保留）
    pub default_quality: u32,
    /// 默认音轨：normal / dolby / flac
    /// （旧字段：新装或迁移后由 `audio_prefs` 承担，仅为读旧配置保留）
    pub default_audio: String,
    /// 画质优先顺序：逐条尝试，命中即用；都没命中时按 `quality_fallback` 处理
    pub quality_prefs: Vec<QualityPref>,
    /// 音轨优先顺序：auto（最佳可用）/ flac / dolby / normal
    pub audio_prefs: Vec<String>,
    /// 可选 HTTP 代理，留空即直连
    pub proxy: String,
    /// 外观：light / dark / system
    pub theme: String,
    /// 首次启动引导是否已完成（老配置缺这个字段时 serde default=false，
    /// 会让老用户也看到一次引导——可接受，之后保存即置 true）
    #[serde(default)]
    pub setup_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_dir: bili_core::login::default_cookie_path().with_file_name("downloads"),
            folder_template: DEFAULT_FOLDER_TEMPLATE.to_string(),
            max_concurrent_tasks: DEFAULT_MAX_CONCURRENT_TASKS,
            chunk_concurrency: 4,
            chunk_mb: 4,
            keep_temp: false,
            naming: "title".to_string(),
            // 默认带批量序号：合集/收藏夹下载按 "01 标题" 落盘，名称排序即列表顺序；
            // 单视频的 {index} 是空值（sanitizer 再 trim 掉前导空格），落成纯标题不受影响
            naming_template: "{index} {title}.{ext}".to_string(),
            naming_presets: Vec::new(),
            folder_presets: Vec::new(),
            rename_conflict: "skip".to_string(),
            container: "mp4".to_string(),
            audio_format: "source".to_string(),
            image_format: "source".to_string(),
            codec_pref: "auto".to_string(),
            quality_fallback: "nearest".to_string(),
            // 封面/弹幕默认勾上：都存独立文件、不影响视频本体，新装用户基本都要
            download_cover: true,
            download_subtitles: false,
            download_danmaku: true,
            embed_cover: false,
            embed_subtitles: false,
            retry_count: 3,
            speed_limit_mib: 0,
            auto_refresh_urls: true,
            resume_on_start: false,
            parse_preset: "标准".to_string(),
            parse_batch: 8,
            parse_batch_wait_ms: 1000,
            parse_rest_every: 100,
            parse_rest_ms: 3000,
            parse_cap: 0,
            ffmpeg_path: String::new(),
            update_check: false,
            log_level: "info".to_string(),
            data_dir: String::new(),
            default_quality: 0,
            default_audio: "auto".to_string(),
            // 优先顺序表**默认空着**：媒体页上面那两个单值（视频清晰度 / 音频质量）就是默认，
            // 空表时挑流走单值那条路。用户真要自定义顺序，才在媒体页往里加行。
            quality_prefs: Vec::new(),
            audio_prefs: Vec::new(),
            proxy: String::new(),
            theme: "system".to_string(),
            setup_done: false,
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        bili_core::login::default_cookie_path().with_file_name("settings.json")
    }

    /// 读取设置；文件不存在或损坏时回退到默认值。
    pub fn load() -> Self {
        let text = std::fs::read_to_string(Self::path()).unwrap_or_default();
        let mut settings: Settings = serde_json::from_str(&text).unwrap_or_default();
        settings.migrate();
        settings.clamp();
        settings
    }

    /// 旧版设置迁移：把 naming 枚举换算成命名模板。
    pub fn migrate(&mut self) {
        if self.naming_template.trim().is_empty() {
            self.naming_template = match self.naming.as_str() {
                "title_quality" => "{title}_{quality}".to_string(),
                "title_bvid" => "{title}_{bvid}".to_string(),
                _ => "{title}.{ext}".to_string(),
            };
        }
        // 老版本把"默认命名"写成裸 `{title}`：渲染结果一样（渲染器会补扩展名），
        // 但和「默认（默认）」这个内置预设的模板对不上，界面只能显示"自定义模板"。
        // 等价改写，让默认能被认出来。
        if self.naming_template.trim() == "{title}" {
            self.naming_template = "{title}.{ext}".to_string();
        }

        // 封面/字幕从"嵌进 MKV"改成"独立文件"：勾过的人就是想要这份东西，搬过来
        if self.embed_cover {
            self.download_cover = true;
        }
        if self.embed_subtitles {
            self.download_subtitles = true;
        }

        // fdc185a 那一版把单值（视频清晰度 / 编码偏好 / 音频质量）折进了优先顺序表，
        // 因为那时界面上只剩表、没有单值。现在单值自己就是媒体页上面的两个下拉，
        // 表反而是"自定义"——折出来的那一行留着会让表非空、把单值选择顶掉。
        // 当年折出来的行与单值完全等价，认出来清掉即可：即便用户自己加过一模一样的行，
        // 清掉后挑流结果也分毫不差（同一个档位 + 同一个编码）。
        let implied = QualityPref {
            qn: if self.default_quality == 0 {
                127
            } else {
                self.default_quality
            },
            codec: self.codec_pref.clone(),
        };
        if self.quality_prefs.len() == 1 && self.quality_prefs[0] == implied {
            self.quality_prefs.clear();
        }
        // 音轨还多一种情形：老版本的默认行是 ["auto"]，而老版本的单值默认是 "normal"，
        // 两者天生对不上，所以"只等于单值"这条认不出它，得把老默认行也认掉。
        // 清掉后走单值：auto 与 normal 在挑流那边都是"普通音轨里取最好的一条"，结果一样。
        if self.audio_prefs.len() == 1
            && (self.audio_prefs[0] == self.default_audio || self.audio_prefs[0] == "auto")
        {
            self.audio_prefs.clear();
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
    }

    /// 把数值收敛到安全范围，避免界面传入异常值把程序搞坏。
    pub fn clamp(&mut self) {
        self.max_concurrent_tasks = self.max_concurrent_tasks.clamp(1, 5);
        self.chunk_concurrency = self.chunk_concurrency.clamp(1, 16);
        self.chunk_mb = self.chunk_mb.clamp(1, 32);
        self.retry_count = self.retry_count.clamp(0, 10);
        self.parse_batch = self.parse_batch.clamp(1, 30);
        self.parse_batch_wait_ms = self.parse_batch_wait_ms.clamp(0, 10_000);
        self.parse_rest_every = self.parse_rest_every.clamp(10, 500);
        self.parse_rest_ms = self.parse_rest_ms.clamp(0, 30_000);
        self.parse_cap = self.parse_cap.min(20_000);
        if self.naming_template.trim().is_empty() {
            self.naming_template = "{title}.{ext}".to_string();
        }
        self.folder_template = self.folder_template.trim().to_string();
        if self.folder_template.chars().count() > 300 {
            self.folder_template = self.folder_template.chars().take(300).collect();
        }
        self.naming_presets.truncate(50);
        for preset in &mut self.naming_presets {
            preset.name = preset.name.trim().to_string();
            preset.template = preset.template.trim().to_string();
        }
        self.naming_presets
            .retain(|preset| !preset.name.is_empty() && !preset.template.is_empty());
        // 文件夹预设和命名预设差在一点：空模板是合法的（= 不建文件夹），
        // 所以只按名字过滤，别把"不建文件夹"这个预设吞掉
        self.folder_presets.truncate(50);
        for preset in &mut self.folder_presets {
            preset.name = preset.name.trim().to_string();
            preset.template = preset.template.trim().to_string();
        }
        self.folder_presets.retain(|preset| !preset.name.is_empty());
        if !matches!(self.rename_conflict.as_str(), "skip" | "overwrite" | "auto") {
            self.rename_conflict = "skip".to_string();
        }
        if !matches!(self.container.as_str(), "mp4" | "ts") {
            // 老设置里的 mkv 也落这里：改成 mp4（MKV 的用途已被独立文件取代）
            self.container = "mp4".to_string();
        }
        if !matches!(self.audio_format.as_str(), "source" | "mp3") {
            self.audio_format = "source".to_string();
        }
        if !matches!(self.image_format.as_str(), "source" | "jpg") {
            self.image_format = "source".to_string();
        }
        if !matches!(self.codec_pref.as_str(), "auto" | "avc" | "hevc" | "av1") {
            self.codec_pref = "auto".to_string();
        }
        if !matches!(self.quality_fallback.as_str(), "nearest" | "fail") {
            self.quality_fallback = "nearest".to_string();
        }
        // auto（最佳可用）与 normal（普通音轨）在挑流端等价，但都是界面下拉里的合法选择
        if !matches!(
            self.default_audio.as_str(),
            "auto" | "normal" | "dolby" | "flac"
        ) {
            self.default_audio = "normal".to_string();
        }
        // 优先顺序列表：**空着是合法的**（= 没自定义，用媒体页上面的视频清晰度 / 音频质量），
        // 这里只管条数与取值收敛
        self.quality_prefs.truncate(12);
        for pref in &mut self.quality_prefs {
            if pref.qn != 0
                && !matches!(
                    pref.qn,
                    6 | 16 | 32 | 64 | 74 | 80 | 100 | 112 | 116 | 120 | 125 | 126 | 127
                )
            {
                pref.qn = 127;
            }
            if !matches!(pref.codec.as_str(), "auto" | "avc" | "hevc" | "av1") {
                pref.codec = "auto".to_string();
            }
        }
        self.audio_prefs.truncate(12);
        for kind in &mut self.audio_prefs {
            if !matches!(kind.as_str(), "auto" | "flac" | "dolby" | "normal") {
                *kind = "auto".to_string();
            }
        }
        if !matches!(self.log_level.as_str(), "debug" | "info" | "warn" | "error") {
            self.log_level = "info".to_string();
        }
        if !matches!(self.theme.as_str(), "light" | "dark" | "system") {
            self.theme = "system".to_string();
        }
        // 0 表示自动；其余限定在已知档位里
        if self.default_quality != 0
            && !matches!(
                self.default_quality,
                6 | 16 | 32 | 64 | 74 | 80 | 100 | 112 | 116 | 120 | 125 | 126 | 127
            )
        {
            self.default_quality = 0;
        }
    }

    /// 输出文件扩展名跟随封装格式。
    pub fn container_ext(&self) -> &'static str {
        if self.container == "ts" {
            "ts"
        } else {
            "mp4"
        }
    }

    /// 文件夹层级：把 folder_template 渲染成目录前缀；模板为空表示不建层级。
    pub fn output_folder_template(&self, ctx: &crate::naming::NamingContext) -> PathBuf {
        if self.folder_template.trim().is_empty() {
            return PathBuf::new();
        }
        crate::naming::render_dir(&self.folder_template, ctx)
    }

    /// 按命名模板渲染输出文件名（不含目录）。
    /// 按命名模板渲染输出相对路径（可含子目录）。
    ///
    /// 变量清单与渲染规则都在 [`crate::naming`]，界面面板由同一份清单生成。
    pub fn output_filename(&self, ctx: &crate::naming::NamingContext) -> PathBuf {
        crate::naming::render(&self.naming_template, ctx, self.container_ext())
    }

    /// 同 [`output_filename`]，但用指定的扩展名（音频固定 m4a，不走封装设置）。
    pub fn output_filename_with_ext(
        &self,
        ctx: &crate::naming::NamingContext,
        ext: &str,
    ) -> PathBuf {
        crate::naming::render(&self.naming_template, ctx, ext)
    }

    /// 图文/专栏的条目文件夹名：**由命名模板渲染** —— 编号之类的规则都归命名规则管。
    ///
    /// 与视频文件名的区别只是"不补扩展名、空壳段落丢掉"，所以文件名模板怎么写，
    /// 条目文件夹就怎么叫：想加编号就在模板里写 `{index}`（宽度由前端按批次给）。
    pub fn output_folder(&self, ctx: &crate::naming::NamingContext) -> PathBuf {
        let path = crate::naming::render_dir(&self.naming_template, ctx);
        if path.as_os_str().is_empty() {
            PathBuf::from("图文")
        } else {
            path
        }
    }

    /// 默认数据目录（凭据、日志、下载默认都在这下面）。
    pub fn default_data_dir() -> PathBuf {
        bili_core::login::default_cookie_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// 生效的数据目录：设置里填了就用它，否则用默认。
    pub fn data_root(&self) -> PathBuf {
        let custom = self.data_dir.trim();
        if custom.is_empty() {
            Self::default_data_dir()
        } else {
            PathBuf::from(custom)
        }
    }

    /// 登录凭据放哪 —— 跟着数据目录走，所以「数据目录」一改，凭据也跟着搬。
    pub fn cookies_path(&self) -> PathBuf {
        self.data_root().join("cookies.json")
    }

    /// 日志目录：同样跟着数据目录。
    pub fn logs_dir(&self) -> PathBuf {
        self.data_root().join("logs")
    }

    /// 按配置级别写一行任务日志；失败静默（日志不该反过来影响下载）。
    pub fn log(&self, level: &str, message: &str) {
        let rank = |level: &str| match level {
            "debug" => 0,
            "info" => 1,
            "warn" => 2,
            _ => 3,
        };
        if rank(level) < rank(&self.log_level) {
            return;
        }
        let dir = self.logs_dir();
        let _ = std::fs::create_dir_all(&dir);
        let line = format!(
            "[{}] [{}] {}\n",
            chrono_now(),
            level.to_ascii_uppercase(),
            message
        );
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("bilidown.log"))
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }
}

/// 本地时间戳，格式 YYYY-MM-DD HH:MM:SS。
fn chrono_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 简化的本地时间：用 UTC 秒数 + 8 小时（用户时区为东八区）
    let secs = secs + 8 * 3600;
    let days = secs / 86400;
    let tod = secs % 86400;
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// 从 Unix 天数算出公历日期（Howard Hinnant 算法）。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

impl AppState {
    pub fn new() -> anyhow::Result<Self> {
        let settings = Settings::load();
        let client = Self::build_client(&settings)?;

        Ok(Self {
            client: RwLock::new(Arc::new(client)),
            tasks: Mutex::new(HashMap::new()),
            slots: RwLock::new(Arc::new(Semaphore::new(settings.max_concurrent_tasks))),
            settings: Mutex::new(settings),
            counter: Mutex::new(0),
            ffmpeg: Mutex::new(None),
            batches: Mutex::new(HashMap::new()),
            warmed: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// 取出某个来源的加载缓存（取出后由调用方持有，避免克隆整份清单）。
    pub fn take_batch(&self, key: &str) -> Option<BatchCache> {
        self.batches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key)
    }

    /// 读一份缓存副本（缓存本体留着不动），用于按序号加载时取来源标题与总数。
    pub fn peek_batch(&self, key: &str) -> Option<BatchCache> {
        self.batches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    pub fn put_batch(&self, key: String, cache: BatchCache) {
        let mut guard = self.batches.lock().unwrap_or_else(|e| e.into_inner());
        if !guard.contains_key(&key) && guard.len() >= MAX_BATCH_CACHE {
            if let Some(victim) = guard.keys().next().cloned() {
                guard.remove(&victim);
            }
        }
        guard.insert(key, cache);
    }

    /// 读缓存的 ffmpeg 探测结果；没有缓存时返回 None（调用方给中性文案，不谎报）。
    pub fn ffmpeg_cached(&self) -> Option<FfmpegStatus> {
        self.ffmpeg
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_ffmpeg_status(&self, status: FfmpegStatus) -> FfmpegStatus {
        *self.ffmpeg.lock().unwrap_or_else(|e| e.into_inner()) = Some(status.clone());
        status
    }

    /// 建会话并装上已保存的登录态。
    fn build_client(settings: &Settings) -> anyhow::Result<BiliClient> {
        let client = BiliClient::with_proxy(Some(&settings.proxy))?;
        // 凭据跟着「数据目录」走（默认目录就是 B 站数据目录旁边那份 cookies.json）
        if let Some(cookies) = bili_core::login::Cookies::load(&settings.cookies_path())? {
            client.set_cookies(&cookies)?;
        }
        Ok(client)
    }

    /// 首次调用时访问一次首页，把 buvid3 之类的风控 Cookie 放进会话（只做一次）。
    pub async fn warmup_once(&self) {
        use std::sync::atomic::Ordering;
        if self.warmed.swap(true, Ordering::SeqCst) {
            return;
        }
        let client = self.client();
        if let Err(e) = client.warmup().await {
            eprintln!("启动预热失败（不影响使用）: {e}");
        }
    }

    pub fn client(&self) -> Arc<BiliClient> {
        self.client
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn slots(&self) -> Arc<Semaphore> {
        self.slots.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 换一个干净的会话（登出、或改了代理之后重建）。
    pub fn reset_client(&self) -> anyhow::Result<()> {
        let settings = self.settings();
        let fresh = Arc::new(Self::build_client(&settings)?);
        *self.client.write().unwrap_or_else(|e| e.into_inner()) = fresh;
        Ok(())
    }

    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 覆盖设置并落盘；返回生效后的设置。
    pub fn apply_settings(&self, mut next: Settings) -> Settings {
        next.clamp();
        // 数据目录换了：把现有登录凭据复制过去（目标已有就不动，旧目录保留）
        let previous_data = self
            .settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cookies_path();
        let next_data = next.cookies_path();
        if previous_data != next_data && previous_data.exists() && !next_data.exists() {
            if let Some(parent) = next_data.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::copy(&previous_data, &next_data) {
                Ok(_) => next.log(
                    "info",
                    &format!("数据目录已更换，登录凭据已复制到 {}", next_data.display()),
                ),
                Err(e) => next.log(
                    "warn",
                    &format!("换数据目录时复制凭据失败（重新登录即可）: {e}"),
                ),
            }
        }
        let previous = {
            let mut guard = self.settings.lock().unwrap_or_else(|e| e.into_inner());
            let previous = guard.clone();
            *guard = next.clone();
            previous
        };

        if let Err(e) = next.save() {
            eprintln!("保存设置失败: {e}");
        }
        if next.max_concurrent_tasks != previous.max_concurrent_tasks {
            *self.slots.write().unwrap_or_else(|e| e.into_inner()) =
                Arc::new(Semaphore::new(next.max_concurrent_tasks));
        }
        if next.proxy != previous.proxy {
            if let Err(e) = self.reset_client() {
                eprintln!("应用代理设置失败: {e}");
            }
        }
        next
    }

    pub fn next_task_id(&self) -> String {
        let mut counter = self.counter.lock().unwrap_or_else(|e| e.into_inner());
        *counter += 1;
        format!("task-{}", *counter)
    }

    pub fn output_dir(&self) -> PathBuf {
        self.settings().output_dir
    }

    pub fn set_output_dir(&self, dir: &Path) {
        let mut settings = self.settings();
        settings.output_dir = dir.to_path_buf();
        self.apply_settings(settings);
    }

    pub fn cookies_path(&self) -> PathBuf {
        self.settings().cookies_path()
    }
}

#[cfg(test)]
mod preset_tests {
    use super::*;

    /// 内置预设「批量带序号（默认）」的模板就是后端的默认值——点「恢复默认」之后
    /// 下拉要能认出它（认不出的表现是显示"自定义模板"）。
    #[test]
    fn default_naming_template_is_the_default_preset() {
        assert_eq!(Settings::default().naming_template, "{index} {title}.{ext}");
    }

    /// 老设置里的裸 `{title}` 会被等价改写成 `{title}.{ext}`（渲染结果完全一样）。
    #[test]
    fn legacy_bare_title_template_is_normalised() {
        let mut settings = Settings {
            naming_template: "{title}".to_string(),
            ..Settings::default()
        };
        settings.migrate();
        assert_eq!(settings.naming_template, "{title}.{ext}");
    }

    /// 命名预设和文件夹预设过滤规则不同：命名模板是文件名，空模板没有意义；
    /// 文件夹模板空着恰恰是"不建文件夹"，是个正当的预设。曾经共用一个 retain
    /// 会把这类预设悄悄吞掉。
    #[test]
    fn folder_presets_may_be_empty_but_naming_presets_may_not() {
        let mut settings = Settings {
            naming_presets: vec![
                NamingPreset {
                    name: "空模板".into(),
                    template: "   ".into(),
                },
                NamingPreset {
                    name: "  ".into(),
                    template: "{title}.{ext}".into(),
                },
                NamingPreset {
                    name: "留下".into(),
                    template: "{title}.{ext}".into(),
                },
            ],
            folder_presets: vec![
                NamingPreset {
                    name: "不建文件夹".into(),
                    template: String::new(),
                },
                NamingPreset {
                    name: "  ".into(),
                    template: "{owner_name}".into(),
                },
                NamingPreset {
                    name: "留下".into(),
                    template: " {owner_name} ".into(),
                },
            ],
            ..Settings::default()
        };

        settings.clamp();

        assert_eq!(
            settings.naming_presets.len(),
            1,
            "空模板的命名预设应当被丢掉"
        );
        assert_eq!(settings.naming_presets[0].name, "留下");
        assert_eq!(settings.folder_presets.len(), 2, "空模板的文件夹预设要留下");
        assert_eq!(settings.folder_presets[0].name, "不建文件夹");
        assert_eq!(settings.folder_presets[0].template, "");
        assert_eq!(
            settings.folder_presets[1].template, "{owner_name}",
            "两端空白要清掉"
        );
    }
}

#[cfg(test)]
mod pref_tests {
    use super::*;

    /// 三个格式选择：默认都是"原样"，MKV 落回 MP4，非法值收敛到默认。
    #[test]
    fn format_settings_clamp() {
        let s = Settings::default();
        assert_eq!(s.container, "mp4");
        assert_eq!(s.audio_format, "source");
        assert_eq!(s.image_format, "source");

        let mut s = Settings {
            container: "mkv".to_string(), // 老设置里的 MKV：MKV 已从界面去掉
            audio_format: "flac".to_string(),
            image_format: "png".to_string(),
            ..Settings::default()
        };
        s.clamp();
        assert_eq!(s.container, "mp4", "mkv 要落回 mp4");
        assert_eq!(s.audio_format, "source");
        assert_eq!(s.image_format, "source");

        let mut s = Settings {
            container: "ts".to_string(),
            audio_format: "mp3".to_string(),
            image_format: "jpg".to_string(),
            ..Settings::default()
        };
        s.clamp();
        assert_eq!(s.container_ext(), "ts");
        assert_eq!(s.audio_format, "mp3");
        assert_eq!(s.image_format, "jpg");
    }

    /// 媒体页的默认状态：优先顺序表空着，挑流用上面两个单值。
    #[test]
    fn priority_lists_start_empty() {
        let s = Settings::default();
        assert!(s.quality_prefs.is_empty(), "默认不该预置优先顺序行");
        assert!(s.audio_prefs.is_empty());
        assert_eq!(s.default_quality, 0, "0 = 最优画质");
        assert_eq!(s.default_audio, "auto", "auto = 最佳可用");
    }

    /// 空表是合法状态（= 未自定义），clamp 不许把它填回默认行。
    #[test]
    fn clamp_does_not_refill_empty_lists() {
        let mut s = Settings::default();
        s.clamp();
        assert!(s.quality_prefs.is_empty());
        assert!(s.audio_prefs.is_empty());
    }

    /// fdc185a 把单值折成了单行表；那一行与单值完全等价，读回时清掉，
    /// 让界面回到"未自定义"，单值下拉重新说了算。
    #[test]
    fn legacy_single_row_is_unfolded() {
        let mut s = Settings {
            default_quality: 80,
            codec_pref: "hevc".to_string(),
            quality_prefs: vec![QualityPref {
                qn: 80,
                codec: "hevc".to_string(),
            }],
            default_audio: "flac".to_string(),
            audio_prefs: vec!["flac".to_string()],
            ..Settings::default()
        };
        s.migrate();
        assert!(s.quality_prefs.is_empty());
        assert!(s.audio_prefs.is_empty());
        assert_eq!(s.default_quality, 80, "单值本身不动");
        assert_eq!(s.codec_pref, "hevc");
        assert_eq!(s.default_audio, "flac");
    }

    /// 真正自定义过的顺序（两行以上）不能被清掉。
    #[test]
    fn real_custom_order_is_kept() {
        let mut s = Settings {
            quality_prefs: vec![
                QualityPref {
                    qn: 80,
                    codec: "avc".to_string(),
                },
                QualityPref {
                    qn: 64,
                    codec: "auto".to_string(),
                },
            ],
            audio_prefs: vec!["flac".to_string(), "auto".to_string()],
            ..Settings::default()
        };
        s.migrate();
        assert_eq!(s.quality_prefs.len(), 2);
        assert_eq!(s.audio_prefs.len(), 2);
    }
}
