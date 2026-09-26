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
    /// 自定义 ffmpeg 路径，留空自动发现
    pub ffmpeg_path: String,
    /// 启动时静默检查更新
    pub update_check: bool,
    /// 任务日志级别：debug / info / warn / error
    pub log_level: String,
    /// 数据目录（日志等），留空用默认数据目录
    pub data_dir: String,
    /// 默认清晰度，0 表示自动取可用最高档
    pub default_quality: u32,
    /// 默认音轨：normal / dolby / flac
    pub default_audio: String,
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
            ffmpeg_path: String::new(),
            update_check: false,
            log_level: "info".to_string(),
            data_dir: String::new(),
            default_quality: 0,
            default_audio: "normal".to_string(),
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
        if self.naming_template.trim().is_empty() {
            self.naming_template = "{title}".to_string();
        }
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
    pub fn output_filename(
        &self,
        title: &str,
        bvid: &str,
        quality_label: &str,
        owner: &str,
    ) -> String {
        let render = |token: &str| match token {
            "title" => bili_core::sanitize_filename(title),
            "bvid" => bvid.to_string(),
            "quality" => bili_core::sanitize_filename(quality_label),
            "owner" => bili_core::sanitize_filename(owner),
            other => format!("{{{other}}}"),
        };

        let mut out = String::new();
        let mut rest = self.naming_template.as_str();
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let after = &rest[start..];
            match after.find('}') {
                Some(end) => {
                    out.push_str(&render(&after[1..end]));
                    rest = &after[end + 1..];
                }
                None => {
                    out.push_str(after);
                    rest = "";
                }
            }
        }
        out.push_str(rest);

        let cleaned = bili_core::sanitize_filename(&out);
        format!("{}.{}", cleaned, self.container_ext())
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
        })
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
