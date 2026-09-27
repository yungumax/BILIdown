//! DASH 流下载引擎：先探测总长度，再按分片并发下载（每片独立重试），
//! 写入前预分配文件，各分片写入互不重叠的区间。

use crate::error::{BiliError, Result};
use futures::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

pub const DEFAULT_CONCURRENCY: usize = 4;
pub const DEFAULT_CHUNK_SIZE: u64 = 4 * 1024 * 1024;
pub const DEFAULT_RETRIES: usize = 3;

#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
    /// 平均速率（字节/秒）
    pub speed_bps: f64,
}

pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub concurrency: usize,
    pub chunk_size: u64,
    pub retries: usize,
    /// 全局限速（字节/秒），None 或 0 表示不限速
    pub speed_limit_bps: u64,
    /// 断点续传：已完成分片的记录文件。传了它，已记下的分片就不再重下；
    /// 每下完一个分片追加一行 `offset len`（崩溃时最多丢最后一行，重下那个分片即可）。
    pub resume_log: Option<std::path::PathBuf>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            concurrency: DEFAULT_CONCURRENCY,
            chunk_size: DEFAULT_CHUNK_SIZE,
            retries: DEFAULT_RETRIES,
            speed_limit_bps: 0,
            resume_log: None,
        }
    }
}

/// 跨连接共享的全局限速器。
///
/// 采用"预约时隙"策略：每个分片按字节数预约未来的一段时间窗，超前的连接睡眠等待。
/// 实现简单且在多连接下仍然公平；限速精度为分片级别（秒级平均速率准确）。
#[derive(Debug)]
pub struct Throttle {
    limit_bps: f64,
    next_slot: Mutex<Instant>,
}

impl Throttle {
    /// 限速为 `limit_bps` 字节/秒；传入 0 返回 None（不限速）。
    pub fn new(limit_bps: u64) -> Option<std::sync::Arc<Self>> {
        (limit_bps > 0).then(|| {
            std::sync::Arc::new(Self {
                limit_bps: limit_bps as f64,
                next_slot: Mutex::new(Instant::now()),
            })
        })
    }

    /// 为 `bytes` 字节等待对应的时隙。
    pub async fn acquire(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        let wait_until = {
            let mut next = self.next_slot.lock().unwrap_or_else(|e| e.into_inner());
            let start = (*next).max(Instant::now());
            *next = start + Duration::from_secs_f64(bytes as f64 / self.limit_bps);
            start
        };
        let now = Instant::now();
        if wait_until > now {
            tokio::time::sleep(wait_until - now).await;
        }
    }
}

/// 下载一个流到 `dest`。`url` 失败时依次尝试 `backup_urls`。
pub async fn download(
    http: &Client,
    url: &str,
    backup_urls: &[String],
    dest: &Path,
    opts: &DownloadOptions,
    progress: ProgressFn,
) -> Result<()> {
    download_with_throttle(http, url, backup_urls, dest, opts, progress, None).await
}

/// 带全局限速器的下载入口。
pub async fn download_with_throttle(
    http: &Client,
    url: &str,
    backup_urls: &[String],
    dest: &Path,
    opts: &DownloadOptions,
    progress: ProgressFn,
    throttle: Option<&std::sync::Arc<Throttle>>,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let (chosen_url, total, range_supported) = probe_candidates(http, url, backup_urls).await?;

    let downloaded = Arc::new(AtomicU64::new(0));
    let reporter = spawn_reporter(downloaded.clone(), total, progress.clone());

    let result = if range_supported && total > opts.chunk_size {
        download_chunked(
            http,
            &chosen_url,
            dest,
            total,
            opts,
            throttle,
            downloaded.clone(),
        )
        .await
    } else {
        if let Some(t) = throttle {
            // 顺序下载同样按总量预约时隙
            t.acquire(total).await;
        }
        download_sequential(http, &chosen_url, dest, downloaded.clone()).await
    };

    reporter.abort();
    result?;

    // 补一次终值，让调用方拿到 100%
    progress(Progress {
        downloaded: downloaded.load(Ordering::Relaxed),
        total,
        speed_bps: 0.0,
    });

    if total > 0 {
        let actual = tokio::fs::metadata(dest).await?.len();
        if actual != total {
            return Err(BiliError::IncompleteDownload {
                expected: total,
                actual,
            });
        }
    }
    Ok(())
}

/// 主地址不可用时自动切到备份地址（B 站每个流会返回多个 CDN 地址）。
async fn probe_candidates(
    http: &Client,
    url: &str,
    backup_urls: &[String],
) -> Result<(String, u64, bool)> {
    let mut last_err = None;
    for candidate in std::iter::once(url.to_string()).chain(backup_urls.iter().cloned()) {
        match probe(http, &candidate).await {
            Ok((total, range_supported)) => return Ok((candidate, total, range_supported)),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| BiliError::Unavailable("没有可用的下载地址".into())))
}

/// 用 `Range: bytes=0-0` 探测总长度与是否支持分段。
async fn probe(http: &Client, url: &str) -> Result<(u64, bool)> {
    let resp = http.get(url).header(RANGE, "bytes=0-0").send().await?;
    let status = resp.status();

    if status == StatusCode::PARTIAL_CONTENT {
        let total = resp
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit('/').next())
            .and_then(|s| s.trim().parse::<u64>().ok());
        let _ = resp.bytes().await; // 只有 1 字节
        total
            .map(|t| (t, true))
            .ok_or_else(|| BiliError::Decode("Content-Range 缺少总长度".into()))
    } else if status.is_success() {
        // 不支持 Range：退化为单连接顺序下载
        let total = resp.content_length().unwrap_or(0);
        Ok((total, false))
    } else {
        Err(BiliError::Unavailable(format!(
            "预检请求失败 HTTP {status}"
        )))
    }
}

async fn download_chunked(
    http: &Client,
    url: &str,
    dest: &Path,
    total: u64,
    opts: &DownloadOptions,
    throttle: Option<&std::sync::Arc<Throttle>>,
    downloaded: Arc<AtomicU64>,
) -> Result<()> {
    // 预分配：文件尺寸立即等于总长度，并发写入各占独立区间
    {
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(dest)
            .await?;
        file.set_len(total).await?;
    }

    let semaphore = Arc::new(Semaphore::new(opts.concurrency.max(1)));
    let mut tasks = tokio::task::JoinSet::new();

    // 断点续传：读已完成分片；已下过的直接跳过（并把字节数记进进度）
    let done = read_resume_log(opts.resume_log.as_deref());
    let mut offset: u64 = 0;
    let mut skipped = 0u64;
    while offset < total {
        let end = (offset + opts.chunk_size - 1).min(total - 1);
        let len = end - offset + 1;
        if done.contains(&offset) {
            skipped += len;
            downloaded.fetch_add(len, Ordering::Relaxed);
            offset = end + 1;
            continue;
        }
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| BiliError::Unavailable(format!("并发控制异常: {e}")))?;

        let http = http.clone();
        let url = url.to_string();
        let dest = dest.to_path_buf();
        let downloaded = downloaded.clone();
        let retries = opts.retries;
        let throttle = throttle.cloned();
        let resume_log = opts.resume_log.clone();

        tasks.spawn(async move {
            let _permit = permit;
            let range = format!("bytes={offset}-{end}");
            let data = fetch_range(&http, &url, &range, retries).await?;
            let len = data.len() as u64;
            if let Some(t) = throttle {
                t.acquire(len).await;
            }
            write_at(&dest, offset, data).await?;
            // 先写数据再记账：崩在中间最多重下这一个分片
            if let Some(log) = resume_log.as_deref() {
                append_resume_log(log, offset, len).await;
            }
            downloaded.fetch_add(len, Ordering::Relaxed);
            Ok::<(), BiliError>(())
        });

        offset = end + 1;
    }

    while let Some(joined) = tasks.join_next().await {
        joined.map_err(|e| BiliError::Unavailable(format!("下载任务异常终止: {e}")))??;
    }
    if skipped > 0 {
        eprintln!("续传：跳过已完成的 {skipped} 字节");
    }
    Ok(())
}

/// 读已完成分片的记录（`offset len` 每行一条）。文件不存在或最后一行写坏了都按"没完成"处理。
fn read_resume_log(path: Option<&Path>) -> std::collections::HashSet<u64> {
    let mut done = std::collections::HashSet::new();
    let Some(path) = path else {
        return done;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return done;
    };
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(offset), Some(len)) = (parts.next(), parts.next()) {
            if let (Ok(offset), Ok(len)) = (offset.parse::<u64>(), len.parse::<u64>()) {
                if len > 0 {
                    done.insert(offset);
                }
            }
        }
    }
    done
}

/// 追加一条已完成分片。写失败不影响下载本身（下一次会重下那个分片）。
async fn append_resume_log(path: &Path, offset: u64, len: u64) {
    use tokio::io::AsyncWriteExt;
    if let Ok(mut file) = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
    {
        let _ = file.write_all(format!("{offset} {len}
").as_bytes()).await;
    }
}

/// 取一个分片，失败按指数退避重试。
async fn fetch_range(http: &Client, url: &str, range: &str, retries: usize) -> Result<Vec<u8>> {
    let mut delay = Duration::from_millis(500);
    let mut last_err = BiliError::Unavailable("未知错误".into());

    for attempt in 0..=retries {
        if attempt > 0 {
            tokio::time::sleep(delay).await;
            delay *= 2;
        }
        match http.get(url).header(RANGE, range).send().await {
            Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                Ok(bytes) => return Ok(bytes.to_vec()),
                Err(e) => last_err = BiliError::Http(e),
            },
            Ok(resp) => {
                last_err = BiliError::Unavailable(format!("分片请求失败 HTTP {}", resp.status()))
            }
            Err(e) => last_err = BiliError::Http(e),
        }
    }
    Err(last_err)
}

async fn download_sequential(
    http: &Client,
    url: &str,
    dest: &Path,
    downloaded: Arc<AtomicU64>,
) -> Result<()> {
    let resp = http.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(BiliError::Unavailable(format!("HTTP {}", resp.status())));
    }

    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded.fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    file.flush().await?;
    Ok(())
}

/// 每个分片单独开句柄并 seek 到自己的区间，避免句柄共享与写入交错。
async fn write_at(path: &Path, offset: u64, data: Vec<u8>) -> Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        use std::io::{Seek, SeekFrom, Write};
        let mut file = std::fs::OpenOptions::new().write(true).open(&path)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&data)?;
        Ok(())
    })
    .await
    .map_err(|e| BiliError::Unavailable(format!("写入线程异常: {e}")))?
}

fn spawn_reporter(
    downloaded: Arc<AtomicU64>,
    total: u64,
    progress: ProgressFn,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let start = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let now = downloaded.load(Ordering::Relaxed);
            let elapsed = start.elapsed().as_secs_f64();
            progress(Progress {
                downloaded: now,
                total,
                speed_bps: if elapsed > 0.0 {
                    now as f64 / elapsed
                } else {
                    0.0
                },
            });
        }
    })
}
