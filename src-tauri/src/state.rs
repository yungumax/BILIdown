//! 应用运行期状态：登录会话、任务表、输出目录。

use crate::types::TaskUpdate;
use bili_core::BiliClient;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::Semaphore;

/// 同时进行的下载任务上限，避免多任务互相抢带宽。
pub const MAX_CONCURRENT_TASKS: usize = 2;

pub struct TaskEntry {
    pub snapshot: Arc<Mutex<TaskUpdate>>,
    pub abort: Option<tokio::task::AbortHandle>,
}

pub struct AppState {
    /// 用 `RwLock` 包一层，登出时可以整体换成没有 Cookie 的新会话
    client: RwLock<Arc<BiliClient>>,
    pub tasks: Mutex<HashMap<String, TaskEntry>>,
    settings: Mutex<Settings>,
    pub slots: Arc<Semaphore>,
    counter: Mutex<u64>,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub output_dir: PathBuf,
    pub cookies_path: PathBuf,
}

impl Settings {
    fn settings_path() -> PathBuf {
        bili_core::login::default_cookie_path().with_file_name("settings.json")
    }

    /// 读取设置；文件不存在或损坏时回退到默认值。
    pub fn load() -> Self {
        let cookies_path = bili_core::login::default_cookie_path();
        let mut settings = Self {
            output_dir: cookies_path.with_file_name("downloads"),
            cookies_path,
        };

        let path = Self::settings_path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(dir) = value.get("output_dir").and_then(|v| v.as_str()) {
                    if !dir.trim().is_empty() {
                        settings.output_dir = PathBuf::from(dir);
                    }
                }
            }
        }
        settings
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = serde_json::json!({
            "output_dir": self.output_dir.to_string_lossy(),
        });
        std::fs::write(path, serde_json::to_string_pretty(&body)?)
    }
}

impl AppState {
    pub fn new() -> anyhow::Result<Self> {
        let client = BiliClient::new()?;
        // 有登录态就直接装上，前端一进来就是已登录状态
        if let Some(cookies) =
            bili_core::login::Cookies::load(&bili_core::login::default_cookie_path())?
        {
            client.set_cookies(&cookies)?;
        }

        Ok(Self {
            client: RwLock::new(Arc::new(client)),
            tasks: Mutex::new(HashMap::new()),
            settings: Mutex::new(Settings::load()),
            slots: Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS)),
            counter: Mutex::new(0),
        })
    }

    pub fn client(&self) -> Arc<BiliClient> {
        self.client
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 换一个干净的会话（登出用）。
    pub fn reset_client(&self) -> anyhow::Result<()> {
        let fresh = Arc::new(BiliClient::new()?);
        *self.client.write().unwrap_or_else(|e| e.into_inner()) = fresh;
        Ok(())
    }

    pub fn next_task_id(&self) -> String {
        let mut counter = self.counter.lock().unwrap_or_else(|e| e.into_inner());
        *counter += 1;
        format!("task-{}", *counter)
    }

    pub fn output_dir(&self) -> PathBuf {
        self.settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .output_dir
            .clone()
    }

    pub fn set_output_dir(&self, dir: &Path) {
        let mut settings = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        settings.output_dir = dir.to_path_buf();
        if let Err(e) = settings.save() {
            eprintln!("保存设置失败: {e}");
        }
    }

    pub fn cookies_path(&self) -> PathBuf {
        self.settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cookies_path
            .clone()
    }
}
