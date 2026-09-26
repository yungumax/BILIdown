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
    /// 文件命名：title / title_quality / title_bvid
    pub naming: String,
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
        settings.clamp();
        settings
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
        if !matches!(
            self.naming.as_str(),
            "title" | "title_quality" | "title_bvid"
        ) {
            self.naming = "title".to_string();
        }
        if !matches!(self.default_audio.as_str(), "normal" | "dolby" | "flac") {
            self.default_audio = "normal".to_string();
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

    /// 按命名规则拼出输出文件名。
    pub fn output_filename(&self, title: &str, bvid: &str, quality_label: &str) -> String {
        let base = bili_core::sanitize_filename(title);
        match self.naming.as_str() {
            "title_quality" => format!("{base}_{quality_label}.mp4"),
            "title_bvid" => format!("{base}_{bvid}.mp4"),
            _ => format!("{base}.mp4"),
        }
    }
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
