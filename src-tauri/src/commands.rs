//! 暴露给前端的命令，是界面与 `bili-core` 之间的唯一通道。

use crate::state::{AppState, BatchCache, BatchTarget, TaskEntry};
use crate::types::*;
use base64::Engine;
use bili_core::api::{codec_name, quality_name};
use bili_core::download::{
    download_with_throttle, DownloadOptions, Progress, ProgressFn, Throttle,
};
use bili_core::error::BiliError;
use bili_core::ffmpeg::Container;
use bili_core::login::{self, Cookies, LoginState};
use bili_core::parser::{is_short_link, parse_target, Target};
use bili_core::{ffmpeg, BiliClient};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
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
pub async fn probe_source(
    state: State<'_, AppState>,
    input: String,
) -> Result<ProbeSource, String> {
    let client = state.client();
    probe_one(&client, &state, &input).await
}

/// 继续解析：往后多拉 `want` 条。首次解析只给第一页，避免一上来就拉上千条。
#[tauri::command]
pub async fn probe_more(
    state: State<'_, AppState>,
    input: String,
    want: usize,
) -> Result<ProbeMore, String> {
    let client = state.client();
    probe_more_one(&client, &state, &input, want).await
}

/// 设置页数据：可编辑项加运行环境信息。
#[tauri::command]
pub async fn app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(collect_settings(&state).await)
}

/// 覆盖保存设置；改并发会重建任务槽位，改代理会重建会话。
#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: crate::state::Settings,
) -> Result<AppSettings, String> {
    state.apply_settings(settings);
    Ok(collect_settings(&state).await)
}

async fn collect_settings(state: &AppState) -> AppSettings {
    let cookies_path = state.cookies_path();
    // 不在这里探测 ffmpeg：探测要起子进程（实测约 0.8 秒），而本函数每次读设置、
    // 每次保存设置都会调用，会把启动与主题切换都拖慢近一秒。真实结果改由
    // `ffmpeg_status` 命令异步取，前端拿到后合并进 env 显示。
    let (ffmpeg_ok, ffmpeg_info) = match state.ffmpeg_cached() {
        Some(status) => (status.ok, status.info),
        None => (true, "检测中…".to_string()),
    };

    AppSettings {
        settings: state.settings(),
        cookies_saved: cookies_path.exists(),
        cookies_path: cookies_path.to_string_lossy().to_string(),
        ffmpeg_ok,
        ffmpeg_info,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// 探测 ffmpeg 可用性与版本；结果缓存在 AppState，`refresh` 为真时忽略缓存重探。
#[tauri::command]
pub async fn ffmpeg_status(
    state: State<'_, AppState>,
    refresh: Option<bool>,
) -> Result<crate::state::FfmpegStatus, String> {
    if !refresh.unwrap_or(false) {
        if let Some(cached) = state.ffmpeg_cached() {
            return Ok(cached);
        }
    }

    let explicit = {
        let path = state.settings().ffmpeg_path.trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(path))
        }
    };

    let status = match ffmpeg::find_ffmpeg(explicit.as_deref()) {
        Some(path) => match ffmpeg::probe_version(&path).await {
            Ok(version) => crate::state::FfmpegStatus {
                ok: true,
                info: version,
            },
            Err(e) => crate::state::FfmpegStatus {
                ok: false,
                info: format!("{} 无法执行：{e}", path.display()),
            },
        },
        None => crate::state::FfmpegStatus {
            ok: false,
            info: "未找到 ffmpeg：请安装到 PATH，或放到程序目录下".to_string(),
        },
    };

    Ok(state.set_ffmpeg_status(status))
}

/// 「魔法变量」面板的数据源：界面直接渲染这份清单，不再手写第二份可能和后端脱节的表。
#[tauri::command]
pub async fn naming_variables() -> Result<Vec<crate::types::NamingVariable>, String> {
    Ok(crate::naming::VARIABLES
        .iter()
        .map(|(token, label)| crate::types::NamingVariable {
            token: (*token).to_string(),
            label: (*label).to_string(),
        })
        .collect())
}

/// 文件名预览：与真实落盘共用同一个渲染器，预览不会和结果对不上。
/// `date` / `publish_date` 由前端按本地时区算好传进来。
#[tauri::command]
pub async fn preview_naming(
    state: State<'_, AppState>,
    template: String,
    date: Option<String>,
    publish_date: Option<String>,
    ext: Option<String>,
) -> Result<String, String> {
    let mut ctx = crate::naming::NamingContext::sample();
    ctx.date = date.unwrap_or_default();
    ctx.publish_date = publish_date.unwrap_or_default();
    let ext = ext.unwrap_or_else(|| state.settings().container_ext().to_string());
    // 用 / 显示，和用户在模板里打的保持一致（Windows 的 PathBuf 会显示成 \）
    Ok(crate::naming::render(&template, &ctx, &ext)
        .to_string_lossy()
        .replace('\\', "/"))
}

/// 单次加载上限，避免超大来源被一次拉上千条（界面上可以「继续解析」分批拉）。
const FAV_MAX_ITEMS: usize = 500;
const COLLECTION_MAX_ITEMS: usize = 500;
const SPACE_MAX_ITEMS: usize = 300;

/// 各来源的单页条数，决定「继续解析」一次往后拉多少页。
fn source_page_size(target: BatchTarget) -> usize {
    match target {
        BatchTarget::Fav(_) | BatchTarget::Whole => 20,
        BatchTarget::Space(_) => 30,
        BatchTarget::Collection { .. } => 100,
    }
}

fn source_cap(kind: &str) -> usize {
    match kind {
        "fav" => FAV_MAX_ITEMS,
        "collection" => COLLECTION_MAX_ITEMS,
        _ => SPACE_MAX_ITEMS,
    }
}

/// 批量来源的加载说明。
///
/// `total > loaded` 有三种原因，用户该做的事都不同：
/// 还没拉完（界面上会显示"已加载 N / M"，不必提示）、到了单次上限、
/// 以及部分内容拿不到（失效/受限，再拉也没有）。实测收藏夹就是最后一种：
/// total=130 而实际只能拿到 129 条。
fn load_note(label: &str, total: usize, loaded: usize, cap: usize, exhausted: bool) -> String {
    if loaded >= cap && total > loaded {
        format!("{label}共 {total} 条，已达单次解析上限 {cap} 条")
    } else if exhausted && total > loaded {
        format!("{label}共 {total} 条，其中 {loaded} 条可下载（其余可能已失效或受限）")
    } else {
        String::new()
    }
}

/// 第一页返回的来源信息（后续页不再重复给）。
struct BatchMeta {
    kind: String,
    title: String,
    owner: String,
    total: usize,
}

/// 拉批量来源的一页。`meta` 只有第一页有值。
async fn fetch_batch_page(
    client: &BiliClient,
    target: BatchTarget,
    page: u32,
) -> Result<(Vec<BatchVideo>, Option<BatchMeta>), String> {
    let first = page == 1;
    match target {
        BatchTarget::Fav(fid) => {
            let data = client.fav_list(fid, page).await.map_err(describe)?;
            let meta = first.then(|| BatchMeta {
                kind: "fav".to_string(),
                title: data.info.title.clone(),
                owner: data.info.upper_name.clone(),
                total: data.info.media_count as usize,
            });
            let items = data
                .medias
                .iter()
                .map(|media| BatchVideo {
                    bvid: media.bvid.clone(),
                    cid: media.cid,
                    ep_id: None,
                    title: media.title.clone(),
                    duration: media.duration,
                })
                .collect();
            Ok((items, meta))
        }
        BatchTarget::Collection { mid, sid } => {
            let data = client
                .seasons_archives(mid, sid, page)
                .await
                .map_err(describe)?;
            let meta = first.then(|| BatchMeta {
                kind: "collection".to_string(),
                title: data.meta.name.clone(),
                owner: data
                    .archives
                    .first()
                    .map(|a| a.owner.name.clone())
                    .unwrap_or_default(),
                total: data.meta.total as usize,
            });
            let items = data
                .archives
                .iter()
                .map(|archive| BatchVideo {
                    bvid: archive.bvid.clone(),
                    cid: archive.cid,
                    ep_id: None,
                    title: archive.title.clone(),
                    duration: archive.duration,
                })
                .collect();
            Ok((items, meta))
        }
        BatchTarget::Space(mid) => {
            let data = client.space_archives(mid, page).await.map_err(describe)?;
            let list = data.list.as_ref();
            let meta = first.then(|| BatchMeta {
                kind: "space".to_string(),
                title: format!(
                    "{} 的投稿",
                    list.and_then(|l| l.vlist.first())
                        .map(|v| v.author.clone())
                        .unwrap_or_default()
                ),
                owner: list
                    .and_then(|l| l.vlist.first())
                    .map(|v| v.author.clone())
                    .unwrap_or_default(),
                total: data.page.count as usize,
            });
            let items = list
                .map(|l| {
                    l.vlist
                        .iter()
                        .map(|video| BatchVideo {
                            bvid: video.bvid.clone(),
                            cid: 0,
                            ep_id: None,
                            title: video.title.clone(),
                            duration: parse_mmss(&video.length),
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok((items, meta))
        }
        // 番剧/课程一次给全，没有分页
        BatchTarget::Whole => Ok((Vec::new(), None)),
    }
}

/// 解析批量来源：拉第一页建缓存，顺带探一次可用清晰度/音轨。
async fn start_batch(
    client: &BiliClient,
    target: BatchTarget,
    meta: BatchMeta,
    items: Vec<BatchVideo>,
) -> Result<BatchCache, String> {
    if items.is_empty() {
        return Err(match meta.kind.as_str() {
            "fav" => "收藏夹为空或不可访问".to_string(),
            "collection" => "合集为空或不可访问".to_string(),
            "space" => "该 UP 主没有可访问的投稿，或触发了风控".to_string(),
            _ => "来源没有可访问的内容".to_string(),
        });
    }

    let source = if meta.kind == "bangumi" {
        "bangumi"
    } else if meta.kind == "cheese" {
        "cheese"
    } else {
        "video"
    };
    let (qualities, audios, recommended_quality, best_quality) =
        probe_media_options(client, &items[0], source).await;

    let exhausted = source_page_size(target) > items.len() && items.len() >= meta.total;
    Ok(BatchCache {
        kind: meta.kind,
        target,
        title: meta.title,
        owner: meta.owner,
        total: meta.total,
        items,
        next_page: 2,
        exhausted,
        qualities,
        audios,
        recommended_quality,
        best_quality,
    })
}

/// 继续往后拉，至少再取 `want` 条（到来源末尾或单次上限为止）。
async fn extend_batch(
    client: &BiliClient,
    cache: &mut BatchCache,
    want: usize,
) -> Result<(), String> {
    let cap = source_cap(&cache.kind);
    let before = cache.items.len();
    while cache.items.len() - before < want && !cache.exhausted {
        if cache.items.len() >= cap {
            cache.exhausted = true;
            break;
        }
        let page = cache.next_page;
        let (items, _) = fetch_batch_page(client, cache.target, page).await?;
        if items.is_empty() {
            cache.exhausted = true;
            break;
        }
        cache.items.extend(items);
        cache.next_page = page + 1;
        if cache.items.len() >= cache.total {
            cache.exhausted = true;
        }
    }
    Ok(())
}

/// 由缓存拼出对外的解析结果。
fn batch_to_source(cache: &BatchCache) -> ProbeSource {
    let loaded = cache.items.len();
    let label = match cache.kind.as_str() {
        "fav" => "收藏夹",
        "collection" => "合集",
        "space" => "投稿",
        _ => "来源",
    };
    ProbeSource {
        kind: cache.kind.clone(),
        title: cache.title.clone(),
        owner: cache.owner.clone(),
        cover: String::new(),
        note: load_note(
            label,
            cache.total,
            loaded,
            source_cap(&cache.kind),
            cache.exhausted,
        ),
        bvid: String::new(),
        cid: 0,
        aid: 0,
        owner_mid: 0,
        pubdate: 0,
        // 批量条目没有"分 P"这个概念，part_* 留空由 {index}/{series_title} 承担
        part_index: 0,
        part_title: String::new(),
        duration: 0,
        page_count: 1,
        total: cache.total,
        loaded,
        exhausted: cache.exhausted,
        qualities: cache.qualities.clone(),
        audios: cache.audios.clone(),
        recommended_quality: cache.recommended_quality,
        best_quality: cache.best_quality,
        items: cache.items.clone(),
    }
}

/// 解析一条来源：短链展开 → 识别目标 → 单视频取详情，批量来源拉第一页。
///
/// 批量来源的加载进度存进 AppState，供「继续解析」接着往后拉，不必从第一页重来。
async fn probe_one(
    client: &BiliClient,
    state: &AppState,
    input: &str,
) -> Result<ProbeSource, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("来源为空".to_string());
    }

    let resolved = if is_short_link(trimmed) {
        client.resolve_redirect(trimmed).await.map_err(describe)?
    } else {
        trimmed.to_string()
    };

    let target = parse_target(&resolved).map_err(describe)?;

    match target {
        Target::Bvid(bvid) => probe_video_bvid(client, &bvid).await,
        Target::Aid(aid) => {
            let info = client.video_info_by_aid(aid).await.map_err(describe)?;
            probe_video_bvid(client, &info.bvid).await
        }

        // ---------- 批量来源：只拉第一页 ----------
        Target::FavList(fid) => {
            probe_batch_first_page(client, state, trimmed, BatchTarget::Fav(fid)).await
        }
        Target::Collection { mid, sid } => {
            probe_batch_first_page(client, state, trimmed, BatchTarget::Collection { mid, sid }).await
        }
        Target::Space(mid) => {
            probe_batch_first_page(client, state, trimmed, BatchTarget::Space(mid)).await
        }
        Target::Bangumi { season_id, ep_id } => {
            let (items, meta) = fetch_whole(client, Target::Bangumi { season_id, ep_id }, "bangumi").await?;
            let meta = meta.ok_or_else(|| "来源没有可访问的内容".to_string())?;
            finish_batch(client, state, trimmed, BatchTarget::Whole, meta, items).await
        }
        Target::Cheese(season_id) => {
            let (items, meta) = fetch_whole(client, Target::Cheese(season_id), "cheese").await?;
            let meta = meta.ok_or_else(|| "来源没有可访问的内容".to_string())?;
            finish_batch(client, state, trimmed, BatchTarget::Whole, meta, items).await
        }
    }
}

/// 分页类批量来源：拉第一页 → 建缓存 → 返回。
async fn probe_batch_first_page(
    client: &BiliClient,
    state: &AppState,
    key: &str,
    target: BatchTarget,
) -> Result<ProbeSource, String> {
    let (items, meta) = fetch_batch_page(client, target, 1).await?;
    let meta = meta.ok_or_else(|| "来源没有可访问的内容".to_string())?;
    finish_batch(client, state, key, target, meta, items).await
}

async fn finish_batch(
    client: &BiliClient,
    state: &AppState,
    key: &str,
    target: BatchTarget,
    meta: BatchMeta,
    items: Vec<BatchVideo>,
) -> Result<ProbeSource, String> {
    let cache = start_batch(client, target, meta, items).await?;
    let source = batch_to_source(&cache);
    state.put_batch(key.to_string(), cache);
    Ok(source)
}

/// 继续解析：从缓存里接着往后拉 `want` 条。
async fn probe_more_one(
    client: &BiliClient,
    state: &AppState,
    input: &str,
    want: usize,
) -> Result<ProbeMore, String> {
    let key = input.trim();
    let mut cache = state
        .take_batch(key)
        .ok_or_else(|| "这个来源的解析结果已过期，请重新解析".to_string())?;

    let before = cache.items.len();
    let result = extend_batch(client, &mut cache, want.max(1)).await;
    let added = cache.items[before..].to_vec();
    let more = ProbeMore {
        items: added,
        loaded: cache.items.len(),
        total: cache.total,
        exhausted: cache.exhausted,
        note: batch_to_source(&cache).note,
    };
    state.put_batch(key.to_string(), cache);
    result.map(|_| more)
}

/// 番剧/课程：一次取全部剧集，包装成第一页。
async fn fetch_whole(
    client: &BiliClient,
    target: Target,
    kind: &str,
) -> Result<(Vec<BatchVideo>, Option<BatchMeta>), String> {
    let (title, items) = match (target, kind) {
        (Target::Bangumi { season_id, ep_id }, _) => {
            let season = client
                .pgc_season(season_id, ep_id)
                .await
                .map_err(describe)?;
            if season.episodes.is_empty() {
                return Err("该番剧没有可访问的剧集".to_string());
            }
            let items = season
                .episodes
                .iter()
                .map(|episode| BatchVideo {
                    bvid: episode.bvid.clone(),
                    cid: episode.cid,
                    ep_id: (episode.id > 0).then_some(episode.id),
                    title: if episode.long_title.is_empty() {
                        episode.title.clone()
                    } else {
                        format!("{} {}", episode.title, episode.long_title)
                    },
                    duration: episode.duration / 1000,
                })
                .collect::<Vec<_>>();
            (season.title, items)
        }
        (Target::Cheese(season_id), _) => {
            let season = client.cheese_season(season_id).await.map_err(describe)?;
            if season.episodes.is_empty() {
                return Err("该课程没有可访问的课时".to_string());
            }
            // 课程 episodes 自带 cid 与秒级时长
            let items = season
                .episodes
                .iter()
                .map(|episode| BatchVideo {
                    bvid: String::new(),
                    cid: episode.cid,
                    ep_id: (episode.id > 0).then_some(episode.id),
                    title: episode.title.clone(),
                    duration: episode.duration,
                })
                .collect::<Vec<_>>();
            (season.title, items)
        }
        _ => return Err("不支持的来源".to_string()),
    };

    let total = items.len();
    Ok((
        items,
        Some(BatchMeta {
            kind: kind.to_string(),
            title,
            owner: String::new(),
            total,
        }),
    ))
}

/// 单视频解析：稿件信息 + playurl 可用档位，包装成统一的 ProbeSource。
async fn probe_video_bvid(client: &BiliClient, bvid: &str) -> Result<ProbeSource, String> {
    let info = client.video_info(bvid).await.map_err(describe)?;

    let play = client
        .playurl(&info.bvid, info.cid, 127)
        .await
        .map_err(describe)?;
    let (qualities, audios, recommended_quality, best_quality) = media_options(&play);

    let note = if info.pages.len() > 1 {
        format!(
            "该视频有 {} 个分 P，当前版本只下载 P1（{}）",
            info.pages.len(),
            info.pages.first().map(|p| p.part.as_str()).unwrap_or("")
        )
    } else {
        String::new()
    };

    // 首页分 P：单 P 视频的 part 常常就是标题，取不到时退回标题
    let first_page = info.pages.first();
    let part_title = first_page
        .map(|p| p.part.clone())
        .filter(|part| !part.is_empty())
        .unwrap_or_else(|| info.title.clone());

    Ok(ProbeSource {
        kind: "video".to_string(),
        title: info.title.clone(),
        owner: info.owner.name.clone(),
        cover: cover_data_url(client, &info.pic).await,
        note,
        bvid: info.bvid.clone(),
        cid: info.cid,
        aid: info.aid,
        owner_mid: info.owner.mid,
        pubdate: info.pubdate,
        part_index: first_page.map(|p| p.page).unwrap_or(1),
        part_title,
        duration: info.duration,
        page_count: info.pages.len(),
        total: 1,
        loaded: 1,
        exhausted: true,
        qualities,
        audios,
        recommended_quality,
        best_quality,
        items: Vec::new(),
    })
}

/// 批量来源的清晰度/音轨探测：用第一条内容按对应来源取 playurl。
async fn probe_media_options(
    client: &BiliClient,
    sample: &BatchVideo,
    source: &str,
) -> (Vec<QualityOption>, Vec<AudioOption>, u32, u32) {
    let play = match source {
        "bangumi" => {
            client
                .pgc_playurl(&sample.bvid, sample.cid, sample.ep_id, 127)
                .await
        }
        "cheese" => match sample.ep_id {
            Some(ep_id) => client.cheese_playurl(ep_id, sample.cid, 127).await,
            None => Err(BiliError::Unavailable("缺少课程 ep_id".into())),
        },
        // 普通视频；空间投稿没有 cid，先按 bvid 补查
        _ => {
            let cid = if sample.cid > 0 {
                sample.cid
            } else {
                client
                    .video_info(&sample.bvid)
                    .await
                    .map(|info| info.cid)
                    .unwrap_or(0)
            };
            client.playurl(&sample.bvid, cid, 127).await
        }
    };

    match play {
        Ok(play) => media_options(&play),
        Err(_) => fallback_media_options(),
    }
}

/// 从 playurl 提炼可选清晰度与音轨。
fn media_options(
    play: &bili_core::api::PlayUrlData,
) -> (Vec<QualityOption>, Vec<AudioOption>, u32, u32) {
    let Some(dash) = play.dash.as_ref() else {
        return fallback_media_options();
    };
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

    (qualities, audios, recommended_quality, best_quality)
}

/// playurl 探测失败时给一个保守的默认档位表。
fn fallback_media_options() -> (Vec<QualityOption>, Vec<AudioOption>, u32, u32) {
    let qualities = vec![
        QualityOption {
            qn: 80,
            label: "高清 1080P".to_string(),
            available: false,
            hint: "需登录".to_string(),
        },
        QualityOption {
            qn: 64,
            label: "高清 720P".to_string(),
            available: false,
            hint: "需登录".to_string(),
        },
        QualityOption {
            qn: 32,
            label: "清晰 480P".to_string(),
            available: true,
            hint: String::new(),
        },
        QualityOption {
            qn: 16,
            label: "流畅 360P".to_string(),
            available: true,
            hint: String::new(),
        },
    ];
    let audios = vec![AudioOption {
        kind: "normal".to_string(),
        label: "普通音轨".to_string(),
        available: true,
    }];
    (qualities, audios, 32, 32)
}

/// UP 空间的 "mm:ss" 时长转秒。
fn parse_mmss(length: &str) -> u64 {
    let mut seconds = 0u64;
    for part in length.split(':') {
        seconds = seconds * 60 + part.trim().parse::<u64>().unwrap_or(0);
    }
    seconds
}

/// 把下载请求 + 本次实际选到的流，拼成命名模板的取值。
///
/// 单独拎出来是为了可测：这里把字段接错（比如 part_index 接了标题）不会报错，
/// 只会静默生成错误的文件名。
fn naming_context(
    req: &DownloadRequest,
    codecs: &str,
    quality: &str,
) -> crate::naming::NamingContext {
    crate::naming::NamingContext {
        title: req.title.clone(),
        part_title: req.naming.part_title.clone(),
        part_index: req.naming.part_index,
        bvid: req.bvid.clone(),
        aid: req.naming.aid,
        cid: req.cid,
        owner_name: req.owner.clone(),
        owner_mid: req.naming.owner_mid,
        series_title: req.naming.series_title.clone(),
        episode_index: req.naming.episode_index,
        episode_title: req.naming.episode_title.clone(),
        collection_title: req.naming.collection_title.clone(),
        index: req.naming.index,
        quality: quality.to_string(),
        codec: codec_name(codecs).to_string(),
        date: req.naming.date.clone(),
        publish_date: req.naming.publish_date.clone(),
    }
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
    let slots = state.slots();
    let settings = state.settings();
    let task_id = id.clone();
    let shared_for_task = shared.clone();
    let task_app = app.clone();

    state
        .settings()
        .log("info", &format!("任务入队: {}（{task_id}）", req.title));

    let handle = tokio::spawn(async move {
        let result = run_download(
            task_app.clone(),
            client,
            slots,
            settings,
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
    settings: crate::state::Settings,
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

    let play = match req.source.as_str() {
        "bangumi" => {
            client
                .pgc_playurl(&req.bvid, req.cid, req.ep_id, req.quality)
                .await?
        }
        "cheese" => {
            let ep_id = req
                .ep_id
                .ok_or_else(|| BiliError::Unavailable("缺少课程 ep_id".to_string()))?;
            client.cheese_playurl(ep_id, req.cid, req.quality).await?
        }
        // 普通视频；空间投稿没有 cid，先按 bvid 补查
        _ => {
            let cid = if req.cid > 0 {
                req.cid
            } else {
                client.video_info(&req.bvid).await?.cid
            };
            client.playurl(&req.bvid, cid, req.quality).await?
        }
    };
    // 候选链：任务里明确选了档位就排在最前，其余按设置里的优先顺序接在后面。
    // 逐条尝试，都没有时按「目标质量不可用」策略处理。
    let mut chain: Vec<(u32, String)> = Vec::new();
    if req.quality > 0 {
        let codec = settings
            .quality_prefs
            .first()
            .map(|pref| pref.codec.clone())
            .unwrap_or_else(|| "auto".to_string());
        chain.push((req.quality, codec));
    }
    for pref in &settings.quality_prefs {
        if pref.qn != req.quality {
            chain.push((pref.qn, pref.codec.clone()));
        }
    }
    if chain.is_empty() {
        chain.push((127, "auto".to_string()));
    }

    let video = play
        .pick_video_chain(&chain, &settings.quality_fallback)
        .ok_or(BiliError::QualityNotFound(req.quality))?;

    // 「目标质量不可用」策略：fail 时请求档位没拿到就直接失败
    if settings.quality_fallback == "fail" && req.quality > 0 && video.id < req.quality {
        settings.log(
            "warn",
            &format!(
                "任务失败：{} 未提供 qn={}（{}）",
                req.title, req.quality, req.bvid
            ),
        );
        return Err(BiliError::QualityNotFound(req.quality));
    }

    let mut audio_chain = settings.audio_prefs.clone();
    if audio_chain.is_empty() {
        audio_chain = vec![req.audio.clone()];
    }
    let audio = play
        .pick_audio_chain(&audio_chain)
        .ok_or_else(|| BiliError::Unavailable("未找到可用音轨".to_string()))?;

    let is_hevc = video.codecs.starts_with("hev") || video.codecs.starts_with("hvc");
    let codec = codec_name(&video.codecs);
    let video_label = format!("{} {}", quality_name(video.id), codec);

    mutate(&shared, &app, |t| {
        t.quality_label = video_label.clone();
    });

    // 目标文件：命名模板 + 封装格式 + 重名处理
    // 模板里的 `/` 会成为子目录，所以还要把中间目录建出来
    let naming = naming_context(req, &video.codecs, quality_name(video.id));    tokio::fs::create_dir_all(&output_dir).await?;
    let out_file = output_dir.join(settings.output_filename(&naming));
    if let Some(parent) = out_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let out_file = match settings.rename_conflict.as_str() {
        "overwrite" => out_file,
        "auto" => find_free_name(out_file).await,
        _ => {
            if out_file.exists() {
                let path = out_file.to_string_lossy().to_string();
                settings.log("info", &format!("文件已存在，跳过任务: {path}"));
                mutate(&shared, &app, |t| {
                    t.status = TaskStatus::Done;
                    t.video_pct = 100.0;
                    t.audio_pct = 100.0;
                    t.output_path = path.clone();
                    t.message = "文件已存在，跳过下载".to_string();
                });
                return Ok(());
            }
            out_file
        }
    };

    let work_key = if req.bvid.is_empty() {
        format!("ep-{}", req.ep_id.unwrap_or(0))
    } else {
        req.bvid.clone()
    };
    let work_dir = output_dir.join(".bilitmp").join(work_key);
    tokio::fs::create_dir_all(&work_dir).await?;
    let video_path = work_dir.join("video.m4s");
    let audio_path = work_dir.join("audio.m4s");
    let opts = DownloadOptions {
        concurrency: settings.chunk_concurrency,
        chunk_size: settings.chunk_mb * 1024 * 1024,
        retries: settings.retry_count as usize,
        speed_limit_bps: settings.speed_limit_mib as u64 * 1024 * 1024,
    };
    let throttle = Throttle::new(opts.speed_limit_bps);

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
        download_with_throttle(
            &client.http,
            &video_url,
            &video_backup,
            &video_path,
            &opts,
            on_progress,
            throttle.as_ref(),
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
        download_with_throttle(
            &client.http,
            &audio_url,
            &audio_backup,
            &audio_path,
            &opts,
            on_progress,
            throttle.as_ref(),
        )
        .await?;
    }

    mutate(&shared, &app, |t| {
        t.status = TaskStatus::Merging;
        t.speed_bps = 0.0;
        t.message = "合成中".to_string();
    });

    let ffmpeg_bin =
        ffmpeg::find_ffmpeg(explicit_ffmpeg(&settings).as_deref()).ok_or_else(|| {
            BiliError::FfmpegUnavailable("未找到 ffmpeg：可在「编码与处理」里指定路径".to_string())
        })?;

    // MKV 且开启嵌入封面时，把解析阶段取到的封面 data URL 落盘
    let container = Container::parse(&settings.container);
    let cover = if container == Container::Mkv && settings.embed_cover && !req.cover.is_empty() {
        match save_cover(&req.cover, &work_dir).await {
            Ok(pair) => Some(pair),
            Err(e) => {
                settings.log("warn", &format!("封面获取失败（继续合成）: {e}"));
                None
            }
        }
    } else {
        None
    };

    ffmpeg::merge_video_audio(
        &ffmpeg_bin,
        &video_path,
        &audio_path,
        &out_file,
        container,
        is_hevc,
        cover
            .as_ref()
            .map(|(path, mime)| (path.as_path(), mime.as_str())),
    )
    .await?;
    if !settings.keep_temp {
        tokio::fs::remove_dir_all(&work_dir).await.ok();
    }

    let out_path = out_file.to_string_lossy().to_string();
    mutate(&shared, &app, |t| {
        t.status = TaskStatus::Done;
        t.video_pct = 100.0;
        t.audio_pct = 100.0;
        t.speed_bps = 0.0;
        t.output_path = out_path.clone();
        t.message = "已完成".to_string();
    });
    settings.log("info", &format!("下载完成: {} -> {out_path}", req.title));

    Ok(())
}

/// 设置里指定了 ffmpeg 路径就交给查找逻辑优先使用。
fn explicit_ffmpeg(settings: &crate::state::Settings) -> Option<PathBuf> {
    let path = settings.ffmpeg_path.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// 「自动重命名」：在 `名字 (n).扩展名` 里找第一个不存在的序号。
async fn find_free_name(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();

    for n in 1..1000u32 {
        let candidate = dir.join(format!("{stem} ({n}).{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path
}

/// 把 data URL 封面落盘，返回 (路径, MIME)。按 data URL 声明的类型决定扩展名。
async fn save_cover(data_url: &str, dir: &Path) -> Result<(PathBuf, String), BiliError> {
    let (mime, b64) = data_url
        .split_once(",")
        .and_then(|(head, payload)| {
            head.strip_prefix("data:")
                .and_then(|head| head.strip_suffix(";base64"))
                .map(|mime| (mime.to_string(), payload))
        })
        .ok_or_else(|| BiliError::Decode("封面 data URL 格式异常".into()))?;

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| BiliError::Decode(format!("封面解码失败: {e}")))?;

    let ext = match mime.as_str() {
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "jpg",
    };
    let path = dir.join(format!("cover.{ext}"));
    tokio::fs::write(&path, &bytes).await?;
    Ok((path, mime))
}

#[tauri::command]
pub fn cancel_download(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> Result<(), String> {
    let (snapshot, bvid, ep_id, abort) = {
        let tasks = state.tasks.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = tasks.get(&task_id) else {
            return Err("任务不存在".to_string());
        };
        let snapshot = entry.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        let bvid = snapshot.bvid.clone();
        let ep_id = snapshot.ep_id;
        (entry.snapshot.clone(), bvid, ep_id, entry.abort.clone())
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

    // 中止后清理它的临时分轨文件（key 与 run_download 保持一致）
    let work_key = if bvid.is_empty() {
        format!("ep-{}", ep_id.unwrap_or(0))
    } else {
        bvid
    };
    std::fs::remove_dir_all(state.output_dir().join(".bilitmp").join(work_key)).ok();
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

/// 前端渲染完成后显示窗口（配合启动隐藏，杜绝首帧闪烁）。
#[tauri::command]
pub fn show_window(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?
        .show()
        .map_err(describe)
}

#[tauri::command]
pub async fn pick_ffmpeg(app: AppHandle) -> Result<String, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("选择 ffmpeg 可执行文件")
        .add_filter("可执行文件", &["exe"])
        .pick_file(move |file| {
            let _ = tx.send(file);
        });

    match rx.await.map_err(describe)? {
        Some(file) => Ok(file.to_string()),
        None => Ok(String::new()),
    }
}

/// 清理下载临时目录（未完成任务的分轨缓存）。
#[tauri::command]
pub async fn cleanup_temp(state: State<'_, AppState>) -> Result<u64, String> {
    let dir = state.output_dir().join(".bilitmp");
    let mut removed: Vec<std::fs::DirEntry> = Vec::new();
    if dir.exists() {
        for entry in std::fs::read_dir(&dir).map_err(describe)? {
            let entry = entry.map_err(describe)?;
            if entry.path().is_dir() {
                removed.push(entry);
            }
        }
    }
    let count = removed.len() as u64;
    for entry in removed {
        std::fs::remove_dir_all(entry.path()).map_err(describe)?;
    }
    state
        .settings()
        .log("info", &format!("清理临时文件 {count} 项"));
    Ok(count)
}

/// 清理网络缓存：重置会话（wbi 密钥等随之重建）。
#[tauri::command]
pub async fn cleanup_cache(state: State<'_, AppState>) -> Result<(), String> {
    state.reset_client().map_err(describe)?;
    state.settings().log("info", "已清理网络缓存（会话重建）");
    Ok(())
}

/// 导出诊断信息到用户选择的文件。
#[tauri::command]
pub async fn export_diagnostics(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let settings = state.settings();
    let ffmpeg_info = match ffmpeg::find_ffmpeg(explicit_ffmpeg(&settings).as_deref()) {
        Some(path) => match ffmpeg::probe_version(&path).await {
            Ok(version) => format!("已就绪：{version}"),
            Err(e) => format!("异常：{e}"),
        },
        None => "未找到".to_string(),
    };

    let task_count = state.tasks.lock().unwrap_or_else(|e| e.into_inner()).len();
    let body = format!(
        "BILIdown 诊断信息
====================
版本: {}
系统: Windows

设置（不含敏感信息）:
{}
ffmpeg: {ffmpeg_info}
任务记录: {task_count} 条
登录凭据存在: {}
",
        env!("CARGO_PKG_VERSION"),
        serde_json::to_string_pretty(&settings).unwrap_or_default(),
        state.cookies_path().exists(),
    );

    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("导出诊断信息")
        .set_file_name("bilidown-diagnostics.txt")
        .save_file(move |file| {
            let _ = tx.send(file);
        });

    match rx.await.map_err(|e| format!("通道异常: {e}"))? {
        Some(file) => {
            let path = file.to_string();
            let path = if path.ends_with(".txt") {
                path
            } else {
                format!("{path}.txt")
            };
            std::fs::write(&path, &body).map_err(describe)?;
            Ok(path)
        }
        None => Ok(String::new()),
    }
}

/// 检查更新：比对 GitHub 最新 Release 与当前版本。
#[derive(serde::Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub up_to_date: bool,
    pub error: String,
}

#[tauri::command]
pub async fn check_updates(state: State<'_, AppState>) -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let client = state.client();
    let resp = client
        .http
        .get("https://api.github.com/repos/yungumax/BILIdown/releases/latest")
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let hint = if status.as_u16() == 404 {
            "仓库不可访问（私有仓库或尚未发布 Release）"
        } else {
            "GitHub 接口不可达"
        };
        return Ok(UpdateInfo {
            current,
            latest: String::new(),
            up_to_date: false,
            error: format!("{hint}（HTTP {status}）"),
        });
    }

    #[derive(serde::Deserialize)]
    struct Release {
        #[serde(rename = "tag_name")]
        tag: String,
    }

    let release: Release = resp
        .json()
        .await
        .map_err(|e| format!("响应解析失败: {e}"))?;
    let latest = release.tag.trim_start_matches('v').to_string();
    let up_to_date = version_cmp(&current, &latest) != std::cmp::Ordering::Less;

    Ok(UpdateInfo {
        current,
        latest,
        up_to_date,
        error: String::new(),
    })
}

/// 简化 semver 比较：逐段按数值比，段数不足补零。
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.')
            .map(|part| part.trim().parse::<u64>().unwrap_or(0))
            .collect()
    };
    let mut left = parse(a);
    let mut right = parse(b);
    let len = left.len().max(right.len());
    left.resize(len, 0);
    right.resize(len, 0);
    left.cmp(&right)
}

#[cfg(test)]
mod naming_tests {
    use super::*;

    fn request() -> DownloadRequest {
        DownloadRequest {
            bvid: "BV1xx411c7mD".to_string(),
            cid: 67890,
            title: "标题".to_string(),
            source: "video".to_string(),
            ep_id: None,
            owner: "UP主".to_string(),
            quality: 116,
            audio: "normal".to_string(),
            cover: String::new(),
            naming: crate::types::NamingMeta {
                part_title: "P1 标题".to_string(),
                part_index: 1,
                aid: 12345,
                owner_mid: 999,
                series_title: "系列".to_string(),
                episode_index: 3,
                episode_title: "第三集".to_string(),
                collection_title: "合集".to_string(),
                index: 7,
                date: "2026-09-26".to_string(),
                publish_date: "2026-01-02".to_string(),
            },
        }
    }

    #[test]
    fn maps_every_field_to_the_right_variable() {
        let ctx = naming_context(&request(), "avc1.640033", "1080P60");

        assert_eq!(ctx.title, "标题");
        assert_eq!(ctx.part_title, "P1 标题");
        assert_eq!(ctx.part_index, 1);
        assert_eq!(ctx.bvid, "BV1xx411c7mD");
        assert_eq!(ctx.aid, 12345);
        assert_eq!(ctx.cid, 67890);
        assert_eq!(ctx.owner_name, "UP主");
        assert_eq!(ctx.owner_mid, 999);
        assert_eq!(ctx.series_title, "系列");
        assert_eq!(ctx.episode_index, 3);
        assert_eq!(ctx.episode_title, "第三集");
        assert_eq!(ctx.collection_title, "合集");
        assert_eq!(ctx.index, 7);
        assert_eq!(ctx.quality, "1080P60");
        assert_eq!(ctx.codec, "AVC");
        assert_eq!(ctx.date, "2026-09-26");
        assert_eq!(ctx.publish_date, "2026-01-02");
    }

    #[test]
    fn codec_comes_from_the_stream_not_the_setting() {
        assert_eq!(naming_context(&request(), "hev1.1.6", "1080P").codec, "HEVC");
        assert_eq!(
            naming_context(&request(), "av01.0.12M.08", "1080P").codec,
            "AV1"
        );
    }

    #[test]
    fn load_note_distinguishes_cap_from_unavailable() {
        // 真的到了单次上限
        let capped = load_note("收藏夹", 900, 500, 500, true);
        assert!(capped.contains("上限"), "{capped}");

        // 还没拉完：界面上的"已加载 N / M"已经说明了，不该提示成"已失效"
        assert!(load_note("收藏夹", 130, 20, 500, false).is_empty());

        // 拉完了却少几条：是有内容拿不到，不能写成"已加载前 N 条"让人以为被截断
        let partial = load_note("收藏夹", 130, 129, 500, true);
        assert!(partial.contains("可下载"), "{partial}");
        assert!(!partial.contains("上限"), "{partial}");

        // 全部拿到：不提示
        assert!(load_note("收藏夹", 129, 129, 500, true).is_empty());
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    fn app_state() -> AppState {
        AppState::new().expect("应用状态")
    }

    fn client() -> BiliClient {
        let client = BiliClient::new().expect("客户端");
        if let Some(cookies) =
            bili_core::login::Cookies::load(&bili_core::login::default_cookie_path()).expect("凭据")
        {
            client.set_cookies(&cookies).expect("装载");
        }
        client
    }

    /// 实测收藏夹解析。cargo test -p bilidown -- --ignored --nocapture live_probe_fav
    #[tokio::test]
    #[ignore = "需要网络与登录态"]
    async fn live_probe_fav() {
        let client = client();
        let probe = probe_one(
            &client,
            &app_state(),
            "https://space.bilibili.com/1858731/favlist?fid=52568231",
        )
        .await
        .expect("解析成功");
        assert_eq!(probe.kind, "fav");
        assert!(probe.loaded > 0, "应加载到视频");
        println!(
            "fav: {} loaded={} total={} 首条={:?}",
            probe.title, probe.loaded, probe.total, probe.items[0].title
        );
    }

    /// 实测 UP 空间解析。
    #[tokio::test]
    #[ignore = "需要网络与登录态"]
    async fn live_probe_space() {
        let client = client();
        let probe = probe_one(&client, &app_state(), "https://space.bilibili.com/1858731")
            .await
            .expect("解析成功");
        assert_eq!(probe.kind, "space");
        assert!(probe.loaded > 0);
        println!(
            "space: {} loaded={} 首条={:?}",
            probe.title, probe.loaded, probe.items[0].title
        );
    }

    /// 实测番剧解析（鲁邦三世 第六季）。
    #[tokio::test]
    #[ignore = "需要网络"]
    async fn live_probe_bangumi() {
        let client = client();
        let probe = probe_one(&client, &app_state(), "https://www.bilibili.com/bangumi/play/ss39468")
            .await
            .expect("解析成功");
        assert_eq!(probe.kind, "bangumi");
        assert!(probe.loaded > 0);
        println!(
            "bangumi: {} loaded={} 清晰度 {:?} 首条={:?}",
            probe.title,
            probe.loaded,
            probe
                .qualities
                .iter()
                .map(|q| (q.qn, q.available))
                .collect::<Vec<_>>(),
            probe.items[0].title
        );
    }

    /// 实测课程解析（公开测试课程 season_id=1）。
    #[tokio::test]
    #[ignore = "需要网络"]
    async fn live_probe_cheese() {
        let client = client();
        let probe = probe_one(&client, &app_state(), "https://www.bilibili.com/cheese/play/ss1")
            .await
            .expect("解析成功");
        assert_eq!(probe.kind, "cheese");
        assert!(probe.loaded > 0);
        println!(
            "cheese: {} loaded={} 首条={:?}",
            probe.title, probe.loaded, probe.items[0].title
        );
    }

    /// 增量加载：首次只给第一页，「继续解析」接着往后拉，不重复也不丢。
    #[tokio::test]
    #[ignore = "需要网络与登录态"]
    async fn live_incremental_loading() {
        let client = client();
        let state = app_state();
        let input = "https://space.bilibili.com/1858731/favlist?fid=52568231";

        let first = probe_one(&client, &state, input).await.expect("首页解析");
        println!(
            "首页 loaded={} total={} exhausted={}",
            first.loaded, first.total, first.exhausted
        );
        assert_eq!(first.loaded, 20, "收藏夹每页 20 条，首页就只该给 20 条");
        assert!(!first.exhausted);

        let more = probe_more_one(&client, &state, input, 50)
            .await
            .expect("继续解析");
        println!(
            "继续 loaded={} 本次新增={} exhausted={}",
            more.loaded,
            more.items.len(),
            more.exhausted
        );
        assert!(more.items.len() >= 50, "应至少再取 50 条");
        assert_eq!(
            more.loaded,
            first.loaded + more.items.len(),
            "进度应等于两次之和"
        );

        // 两次之间不能重复，否则界面上会出现同一集两条
        let first_keys: std::collections::HashSet<&str> =
            first.items.iter().map(|i| i.bvid.as_str()).collect();
        for item in &more.items {
            assert!(
                !first_keys.contains(item.bvid.as_str()),
                "重复条目: {}",
                item.bvid
            );
        }
    }

    /// 分页验证：拿一条真的超过单页的批量来源，确认确实逐页拉全。
    ///
    /// 单页条数是固定的（收藏夹 20、UP 空间 30、合集 100），所以 `loaded` 超过
    /// 单页条数就只可能是翻了页——这比"看代码是循环"有说服力。
    /// UP 空间默认用环境变量 `BILIDOWN_TEST_SPACE_MID` 指定的 mid（需有 30 条以上投稿）。
    #[tokio::test]
    #[ignore = "需要网络与登录态"]
    async fn live_paging_multi_page() {
        let client = client();

        // 收藏夹：每页 20 条
        let fav = probe_one(
            &client,
            &app_state(),
            "https://space.bilibili.com/1858731/favlist?fid=52568231",
        )
        .await
        .expect("收藏夹解析成功");
        println!(
            "fav   loaded={} total={} → 至少翻了 {} 页 | note={:?}",
            fav.loaded,
            fav.total,
            fav.loaded.div_ceil(20),
            fav.note
        );
        assert!(
            fav.loaded > 20,
            "收藏夹只有 {} 条，不足以证明翻页",
            fav.loaded
        );

        // UP 空间：每页 30 条
        if let Ok(mid) = std::env::var("BILIDOWN_TEST_SPACE_MID") {
            let space = probe_one(&client, &app_state(), &format!("https://space.bilibili.com/{mid}"))
                .await
                .expect("空间解析成功");
            println!(
                "space loaded={} total={} → 至少翻了 {} 页",
                space.loaded,
                space.total,
                space.loaded.div_ceil(30)
            );
            assert!(
                space.loaded > 30,
                "空间只有 {} 条，不足以证明翻页（换个投稿更多的 mid）",
                space.loaded
            );
        }

        // 合集：每页 100 条，用 BILIDOWN_TEST_COLLECTION_URL 指定
        if let Ok(url) = std::env::var("BILIDOWN_TEST_COLLECTION_URL") {
            let collection = probe_one(&client, &app_state(), &url).await.expect("合集解析成功");
            println!(
                "coll  loaded={} total={} → 至少翻了 {} 页",
                collection.loaded,
                collection.total,
                collection.loaded.div_ceil(100)
            );
            assert!(
                collection.loaded > 100,
                "合集只有 {} 条，不足以证明翻页（换个条目更多的合集）",
                collection.loaded
            );
        }
    }
}
