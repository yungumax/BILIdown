//! 暴露给前端的命令，是界面与 `bili-core` 之间的唯一通道。

use crate::state::{AppState, TaskEntry};
use crate::types::*;
use base64::Engine;
use bili_core::api::{quality_name, AudioKind};
use bili_core::download::{download, DownloadOptions, Progress, ProgressFn};
use bili_core::error::BiliError;
use bili_core::login::{self, Cookies, LoginState};
use bili_core::parser::{is_short_link, parse_target, Target};
use bili_core::util::sanitize_filename;
use bili_core::{ffmpeg, BiliClient};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::Semaphore;

/// 事件名：任务状态变化，前端据此更新列表。
pub const TASK_EVENT: &str = "task://update";

fn describe(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn percent(done: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        done as f64 / total as f64 * 100.0
    }
}

/// 更新任务快照并把最新状态推给前端。
///
/// 用同步锁是为了能在下载进度回调（同步函数）里直接调用；调用期间不能有 await。
fn mutate<F: FnOnce(&mut TaskUpdate)>(shared: &Arc<Mutex<TaskUpdate>>, app: &AppHandle, f: F) {
    let snapshot = {
        let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard);
        guard.clone()
    };
    let _ = app.emit(TASK_EVENT, snapshot);
}

async fn current_login(client: &BiliClient) -> LoginInfo {
    match client.nav().await {
        Ok(nav) if nav.is_login => LoginInfo {
            logged_in: true,
            uname: nav.uname.clone(),
            mid: nav.mid,
            vip: nav.vip_status > 0,
            vip_label: nav.vip_label_text().to_string(),
        },
        _ => LoginInfo::default(),
    }
}

#[tauri::command]
pub async fn app_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let client = state.client();
    let login = current_login(&client).await;
    Ok(AppStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        login,
        output_dir: state.output_dir().to_string_lossy().to_string(),
        cookies_path: state.cookies_path().to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn probe_video(state: State<'_, AppState>, input: String) -> Result<ProbeResult, String> {
    let client = state.client();

    let input = if is_short_link(&input) {
        client.resolve_redirect(&input).await.map_err(describe)?
    } else {
        input
    };

    let target = parse_target(&input).map_err(describe)?;
    let info = match target {
        Target::Bvid(bvid) => client.video_info(&bvid).await,
        Target::Aid(aid) => client.video_info_by_aid(aid).await,
    }
    .map_err(describe)?;

    // 用最高档请求，一次拿到「内容提供哪些清晰度」和「账号实际能拿到哪些」
    let play = client
        .playurl(&info.bvid, info.cid, 127)
        .await
        .map_err(describe)?;
    let dash = play
        .dash
        .as_ref()
        .ok_or_else(|| "该内容未返回 DASH 流（番剧/课程等暂不支持）".to_string())?;

    let obtainable: Vec<u32> = dash.video.iter().map(|s| s.id).collect();
    let mut qualities: Vec<QualityOption> = play
        .accept_quality
        .iter()
        .zip(play.accept_description.iter())
        .map(|(qn, desc)| QualityOption {
            qn: *qn,
            label: desc.clone(),
            available: obtainable.contains(qn),
            hint: quality_hint(*qn, obtainable.contains(qn)),
        })
        .collect();
    if qualities.is_empty() {
        qualities = obtainable
            .iter()
            .map(|qn| QualityOption {
                qn: *qn,
                label: quality_name(*qn).to_string(),
                available: true,
                hint: String::new(),
            })
            .collect();
    }
    // 从高到低排列，界面下拉里高分档在前
    qualities.sort_by_key(|q| std::cmp::Reverse(q.qn));

    let best_quality = obtainable.iter().copied().max().unwrap_or(0);
    let recommended_quality = if obtainable.contains(&80) {
        80
    } else {
        best_quality
    };

    let best_normal_kbps = dash
        .audio
        .iter()
        .filter(|s| s.id < 30250)
        .map(|s| s.bandwidth / 1000)
        .max()
        .unwrap_or(0);
    let audios = vec![
        AudioOption {
            kind: "normal".to_string(),
            label: format!("普通音轨 {best_normal_kbps} kbps"),
            available: best_normal_kbps > 0,
        },
        AudioOption {
            kind: "dolby".to_string(),
            label: "杜比全景声".to_string(),
            available: dash.dolby.as_ref().and_then(|d| d.audio.as_ref()).is_some(),
        },
        AudioOption {
            kind: "flac".to_string(),
            label: "Hi-Res 无损".to_string(),
            available: dash.flac.as_ref().and_then(|f| f.audio.as_ref()).is_some(),
        },
    ];

    let note = if info.pages.len() > 1 {
        format!(
            "该视频有 {} 个分 P，当前版本只下载 P1（{}）",
            info.pages.len(),
            info.pages.first().map(|p| p.part.as_str()).unwrap_or("")
        )
    } else {
        String::new()
    };

    Ok(ProbeResult {
        bvid: info.bvid.clone(),
        cid: info.cid,
        title: info.title.clone(),
        owner: info.owner.name.clone(),
        duration: info.duration,
        cover: cover_data_url(&client, &info.pic).await,
        page_count: info.pages.len(),
        note,
        qualities,
        audios,
        recommended_quality,
        best_quality,
    })
}

fn quality_hint(qn: u32, available: bool) -> String {
    if available {
        return String::new();
    }
    if qn >= 120 {
        "需大会员".to_string()
    } else if qn >= 64 {
        "需登录".to_string()
    } else {
        String::new()
    }
}

/// 取封面缩略图并转成 data URL，避免 WebView 直连图床时的防盗链问题。
async fn cover_data_url(client: &BiliClient, pic: &str) -> String {
    if pic.is_empty() {
        return String::new();
    }

    // B 站图床支持按尺寸取缩略图，走缩略图可以少传很多数据
    for candidate in [format!("{pic}@168w_168h_1c.webp"), pic.to_string()] {
        let Ok(resp) = client.http.get(&candidate).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(bytes) = resp.bytes().await else {
            continue;
        };
        let mime = if candidate.ends_with(".webp") {
            "image/webp"
        } else if candidate.ends_with(".png") {
            "image/png"
        } else {
            "image/jpeg"
        };
        return format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
    }
    String::new()
}

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    req: DownloadRequest,
) -> Result<String, String> {
    let id = state.next_task_id();
    let mut initial = TaskUpdate::new(id.clone(), &req);
    initial.quality_label = quality_name(req.quality).to_string();
    let shared = Arc::new(Mutex::new(initial.clone()));

    let client = state.client();
    let output_dir = state.output_dir();
    let slots = state.slots.clone();
    let task_id = id.clone();
    let shared_for_task = shared.clone();
    let task_app = app.clone();

    let handle = tokio::spawn(async move {
        let result = run_download(
            task_app.clone(),
            client,
            slots,
            output_dir,
            &req,
            shared_for_task.clone(),
        )
        .await;
        if let Err(e) = result {
            mutate(&shared_for_task, &task_app, |t| {
                if t.status != TaskStatus::Done && t.status != TaskStatus::Canceled {
                    t.status = TaskStatus::Failed;
                    t.message = e.to_string();
                }
            });
        }
    });

    state
        .tasks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            task_id,
            TaskEntry {
                snapshot: shared,
                abort: Some(handle.abort_handle()),
            },
        );

    let _ = app.emit(TASK_EVENT, initial);
    Ok(id)
}

async fn run_download(
    app: AppHandle,
    client: Arc<BiliClient>,
    slots: Arc<Semaphore>,
    output_dir: PathBuf,
    req: &DownloadRequest,
    shared: Arc<Mutex<TaskUpdate>>,
) -> Result<(), BiliError> {
    // 并发槽位不足时在这里排队，状态保持「排队中」
    let _permit = slots
        .acquire()
        .await
        .map_err(|e| BiliError::Unavailable(format!("并发控制异常: {e}")))?;

    mutate(&shared, &app, |t| {
        t.status = TaskStatus::Downloading;
        t.message = "获取播放地址".to_string();
    });

    let play = client.playurl(&req.bvid, req.cid, req.quality).await?;
    let kind = AudioKind::parse(&req.audio).unwrap_or(AudioKind::Normal);
    let video = play
        .pick_video(req.quality, true)
        .ok_or(BiliError::QualityNotFound(req.quality))?;
    let audio = play
        .pick_audio(kind)
        .ok_or_else(|| BiliError::Unavailable("未找到可用音轨".to_string()))?;

    let is_hevc = video.codecs.starts_with("hev") || video.codecs.starts_with("hvc");
    let video_label = format!(
        "{} {}",
        quality_name(video.id),
        if is_hevc { "HEVC" } else { "AVC" }
    );

    mutate(&shared, &app, |t| {
        t.quality_label = video_label.clone();
    });

    let work_dir = output_dir.join(".bilitmp").join(&req.bvid);
    tokio::fs::create_dir_all(&work_dir).await?;
    let video_path = work_dir.join("video.m4s");
    let audio_path = work_dir.join("audio.m4s");
    let opts = DownloadOptions::default();

    let video_url = video.base_url.clone();
    let video_backup = video.backup_url.clone();
    {
        let shared = shared.clone();
        let app = app.clone();
        let on_progress: ProgressFn = Arc::new(move |p: Progress| {
            mutate(&shared, &app, |t| {
                t.status = TaskStatus::Downloading;
                t.message = "下载视频流".to_string();
                t.video_pct = percent(p.downloaded, p.total);
                t.video_bytes = p.downloaded;
                t.video_total = p.total;
                t.speed_bps = p.speed_bps;
                t.recalc();
            });
        });
        download(
            &client.http,
            &video_url,
            &video_backup,
            &video_path,
            &opts,
            on_progress,
        )
        .await?;
    }

    let audio_url = audio.base_url.clone();
    let audio_backup = audio.backup_url.clone();
    {
        let shared = shared.clone();
        let app = app.clone();
        let on_progress: ProgressFn = Arc::new(move |p: Progress| {
            mutate(&shared, &app, |t| {
                t.message = "下载音频流".to_string();
                t.audio_pct = percent(p.downloaded, p.total);
                t.audio_bytes = p.downloaded;
                t.audio_total = p.total;
                t.speed_bps = p.speed_bps;
                t.recalc();
            });
        });
        download(
            &client.http,
            &audio_url,
            &audio_backup,
            &audio_path,
            &opts,
            on_progress,
        )
        .await?;
    }

    mutate(&shared, &app, |t| {
        t.status = TaskStatus::Merging;
        t.speed_bps = 0.0;
        t.message = "合成中".to_string();
    });

    let ffmpeg_bin = ffmpeg::find_ffmpeg(None).ok_or_else(|| {
        BiliError::FfmpegUnavailable("未找到 ffmpeg，请安装到 PATH 或放入程序目录".to_string())
    })?;

    tokio::fs::create_dir_all(&output_dir).await?;
    let out_file = output_dir.join(format!("{}.mp4", sanitize_filename(&req.title)));
    ffmpeg::merge_video_audio(&ffmpeg_bin, &video_path, &audio_path, &out_file, is_hevc).await?;
    tokio::fs::remove_dir_all(&work_dir).await.ok();

    let out_path = out_file.to_string_lossy().to_string();
    mutate(&shared, &app, |t| {
        t.status = TaskStatus::Done;
        t.video_pct = 100.0;
        t.audio_pct = 100.0;
        t.speed_bps = 0.0;
        t.output_path = out_path.clone();
        t.message = "已完成".to_string();
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_download(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<(), String> {
    let (snapshot, bvid, abort) = {
        let tasks = state.tasks.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = tasks.get(&task_id) else {
            return Err("任务不存在".to_string());
        };
        let bvid = entry
            .snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .bvid
            .clone();
        (entry.snapshot.clone(), bvid, entry.abort.clone())
    };

    // 先落状态再中止：任务被 abort 后不会再有后续写入，取消态会保留下来
    mutate(&snapshot, &app, |t| {
        t.status = TaskStatus::Canceled;
        t.message = "已取消".to_string();
        t.speed_bps = 0.0;
    });

    if let Some(handle) = abort {
        handle.abort();
    }

    // 中止后清理它的临时分轨文件
    std::fs::remove_dir_all(state.output_dir().join(".bilitmp").join(&bvid)).ok();
    Ok(())
}

#[tauri::command]
pub async fn login_qrcode(state: State<'_, AppState>) -> Result<QrInfo, String> {
    let client = state.client();
    let qr = client.qrcode_generate().await.map_err(describe)?;
    let _ = std::fs::remove_file(state.cookies_path().with_extension("failed.json"));
    Ok(QrInfo {
        url: qr.url,
        qrcode_key: qr.qrcode_key,
    })
}

#[tauri::command]
pub async fn login_poll(
    state: State<'_, AppState>,
    qrcode_key: String,
) -> Result<LoginPoll, String> {
    let client = state.client();
    let poll = client.qrcode_poll(&qrcode_key).await.map_err(describe)?;
    let state_name = match poll.state() {
        LoginState::Pending => "pending",
        LoginState::Scanned => "scanned",
        LoginState::Expired => "expired",
        LoginState::Confirmed => "confirmed",
    };

    let mut login = LoginInfo::default();
    if poll.state() == LoginState::Confirmed {
        let cookies = login::confirm(&client, &qrcode_key, &poll.url)
            .await
            .map_err(describe)?;
        cookies.save(&state.cookies_path()).map_err(describe)?;
        client.set_cookies(&cookies).map_err(describe)?;
        let _ = client.refresh_wbi_keys().await;
        login = current_login(&client).await;
    }

    Ok(LoginPoll {
        state: state_name.to_string(),
        login,
    })
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Result<LoginInfo, String> {
    Cookies::remove(&state.cookies_path()).map_err(describe)?;
    // 会话里已写入的 Cookie 无法逐条撤销，直接换一个干净的客户端
    state.reset_client().map_err(describe)?;
    let client = state.client();
    let _ = client.warmup().await;
    Ok(LoginInfo::default())
}

#[tauri::command]
pub async fn choose_output_dir(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("选择下载保存目录")
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });

    match rx.await.map_err(describe)? {
        Some(folder) => {
            let dir = folder.to_string();
            state.set_output_dir(std::path::Path::new(&dir));
            Ok(dir)
        }
        None => Ok(state.output_dir().to_string_lossy().to_string()),
    }
}

#[tauri::command]
pub async fn set_output_dir(state: State<'_, AppState>, dir: String) -> Result<String, String> {
    let dir = dir.trim();
    if dir.is_empty() {
        return Err("目录不能为空".to_string());
    }
    std::fs::create_dir_all(dir).map_err(describe)?;
    state.set_output_dir(std::path::Path::new(dir));
    Ok(state.output_dir().to_string_lossy().to_string())
}

#[tauri::command]
pub async fn open_path(path: String) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("路径为空".to_string());
    }
    // explorer 即使成功也会返回非 0 退出码，因此不检查退出状态
    std::process::Command::new("explorer")
        .arg(path.replace('/', "\\"))
        .spawn()
        .map_err(describe)?;
    Ok(())
}
