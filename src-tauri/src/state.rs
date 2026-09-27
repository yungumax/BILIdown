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
    Collection { mid: u64, sid: u64 },
    /// 系列：链接与合集同形，接口不同（见 parser 里的说明）
    Series { mid: u64, sid: u64 },
    /// 图文列表：mid（游标翻页，见 next_offset）
    Opus(u64),
    /// 音频投稿列表：mid
    Audio(u64),
    Space(u64),
    /// 番剧/课程一次给全，没有分页
    Whole,
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

/// 缓存条目上限：同一来源边看边拉时只会有几条，超了丢最早的一条。
const MAX_BATCH_CACHE: usize = 8;

/// 一条画质优先项：目标档位 + 同档内的编码偏好（auto/avc/hevc/av1）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityPref {
    pub qn: u32,
    #[serde(default = "default_codec")]
    pub codec: String,
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
    /// 重名处理：skip=跳过任务 / overwrite=覆盖 / auto=自动加序号
    pub rename_conflict: String,
    /// 封装格式：mp4 / mkv
    pub container: String,
    /// 视频编码偏好：auto / avc / hevc
    pub codec_pref: String,
    /// 请求的清晰度不可用时：nearest=自动降级 / fail=任务失败
    pub quality_fallback: String,
    /// 嵌入封面（仅 MKV 生效）
    pub embed_cover: bool,
    /// 嵌入字幕（仅 MKV；字幕下载在后续版本提供）
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_dir: bili_core::login::default_cookie_path().with_file_name("downloads"),
            max_concurrent_tasks: DEFAULT_MAX_CONCURRENT_TASKS,
            chunk_concurrency: 4,
            chunk_mb: 4,
            keep_temp: false,
            naming: "title".to_string(),
            naming_template: "{title}".to_string(),
            naming_presets: Vec::new(),
            rename_conflict: "skip".to_string(),
            container: "mp4".to_string(),
            codec_pref: "auto".to_string(),
            quality_fallback: "nearest".to_string(),
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
            default_audio: "normal".to_string(),
            // 默认与界面一致：第 1 优先画质 8K、编码不限；第 1 优先音质 最佳可用
            quality_prefs: vec![QualityPref {
                qn: 127,
                codec: "auto".to_string(),
            }],
            audio_prefs: vec!["auto".to_string()],
            proxy: String::new(),
            theme: "system".to_string(),
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
                _ => "{title}".to_string(),
            };
        }

        // 旧的单值偏好迁进优先顺序列表。只在用户确实改过旧字段、且还没动过新列表时做，
        // 否则会把用户在新界面上排好的顺序覆盖掉。
        let default_prefs = Self::default().quality_prefs;
        if self.quality_prefs == default_prefs
            && (self.default_quality != 0 || self.codec_pref != "auto")
        {
            self.quality_prefs = vec![QualityPref {
                qn: if self.default_quality == 0 {
                    127
                } else {
                    self.default_quality
                },
                codec: self.codec_pref.clone(),
            }];
        }
        let default_audio_prefs = Self::default().audio_prefs;
        if self.audio_prefs == default_audio_prefs && self.default_audio != "normal" {
            self.audio_prefs = vec![self.default_audio.clone()];
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
            self.naming_template = "{title}".to_string();
        }
        self.naming_presets.truncate(50);
        for preset in &mut self.naming_presets {
            preset.name = preset.name.trim().to_string();
            preset.template = preset.template.trim().to_string();
        }
        self.naming_presets
            .retain(|preset| !preset.name.is_empty() && !preset.template.is_empty());
        if !matches!(self.rename_conflict.as_str(), "skip" | "overwrite" | "auto") {
            self.rename_conflict = "skip".to_string();
        }
        if !matches!(self.container.as_str(), "mp4" | "mkv") {
            self.container = "mp4".to_string();
        }
        if !matches!(self.codec_pref.as_str(), "auto" | "avc" | "hevc") {
            self.codec_pref = "auto".to_string();
        }
        if !matches!(self.quality_fallback.as_str(), "nearest" | "fail") {
            self.quality_fallback = "nearest".to_string();
        }
        if !matches!(self.default_audio.as_str(), "normal" | "dolby" | "flac") {
            self.default_audio = "normal".to_string();
        }
        // 优先顺序列表：不能为空（空列表等于没有偏好），条数与取值都收敛
        if self.quality_prefs.is_empty() {
            self.quality_prefs = vec![QualityPref {
                qn: 127,
                codec: "auto".to_string(),
            }];
        }
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
        if self.audio_prefs.is_empty() {
            self.audio_prefs = vec!["auto".to_string()];
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
        if self.container == "mkv" {
            "mkv"
        } else {
            "mp4"
        }
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

    /// 按命名模板渲染输出目录（图文这类"一条目一文件夹"用它）。
    pub fn output_folder(&self, ctx: &crate::naming::NamingContext) -> PathBuf {
        crate::naming::render_dir(&self.naming_template, ctx)
    }

    /// 日志目录：自定义数据目录优先，否则用默认数据目录。
    pub fn logs_dir(&self) -> PathBuf {
        let custom = self.data_dir.trim();
        if custom.is_empty() {
            bili_core::login::default_cookie_path().with_file_name("logs")
        } else {
            PathBuf::from(custom).join("logs")
        }
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
        self.ffmpeg.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_ffmpeg_status(&self, status: FfmpegStatus) -> FfmpegStatus {
        *self.ffmpeg.lock().unwrap_or_else(|e| e.into_inner()) = Some(status.clone());
        status
    }

    /// 建会话并装上已保存的登录态。
    fn build_client(settings: &Settings) -> anyhow::Result<BiliClient> {
        let client = BiliClient::with_proxy(Some(&settings.proxy))?;
        if let Some(cookies) = bili_core::login::Cookies::load(&Self::default_cookies_path())? {
            client.set_cookies(&cookies)?;
        }
        Ok(client)
    }

    fn default_cookies_path() -> PathBuf {
        bili_core::login::default_cookie_path()
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
        Self::default_cookies_path()
    }
}
