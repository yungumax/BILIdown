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
    // 启动时预热一次（访问首页拿 buvid3 等风控 Cookie）：字幕、播放地址这些接口
    // 被 412 挡掉，常见原因就是会话里缺这些 Cookie
    state.warmup_once().await;
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
    prefer_collection: Option<bool>,
) -> Result<ProbeSource, String> {
    let client = state.client();
    probe_one(&client, &state, &input, prefer_collection.unwrap_or(false)).await
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

/// 按序号加载：从第 `from` 条开始重新取一批，用于超过单次上限的来源分批下载。
#[tauri::command]
pub async fn probe_range(
    state: State<'_, AppState>,
    input: String,
    from: usize,
) -> Result<ProbeSource, String> {
    let client = state.client();
    probe_range_one(&client, &state, &input, from).await
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

/// 文件名预览：按当前命名模板给每条内容算出文件名。
///
/// 与真正落盘共用 `naming::render`；清晰度用界面所选档位的标准名，
/// 编码取设置里优先顺序中第一个明确指定的（下载时才由实际流决定）。
#[tauri::command]
pub async fn preview_names(
    state: State<'_, AppState>,
    items: Vec<crate::types::NamingPreviewItem>,
    quality: Option<u32>,
    date: Option<String>,
    ext: Option<String>,
) -> Result<Vec<String>, String> {
    let settings = state.settings();
    let ext = ext.unwrap_or_else(|| settings.container_ext().to_string());
    let codec = settings
        .quality_prefs
        .iter()
        .map(|pref| pref.codec.as_str())
        .find(|codec| *codec != "auto")
        .unwrap_or("avc");
    let codec_label = codec_name(match codec {
        "hevc" => "hev1",
        "av1" => "av01",
        _ => "avc1",
    });

    let names = items
        .iter()
        .map(|item| {
            let ctx = crate::naming::NamingContext {
                title: item.title.clone(),
                part_title: item.naming.part_title.clone(),
                part_index: item.naming.part_index,
                bvid: item.bvid.clone(),
                aid: item.naming.aid,
                cid: item.cid,
                owner_name: item.owner.clone(),
                owner_mid: item.naming.owner_mid,
                series_title: item.naming.series_title.clone(),
                episode_index: item.naming.episode_index,
                episode_title: item.naming.episode_title.clone(),
                collection_title: item.naming.collection_title.clone(),
                source_kind: kind_label(&item.kind).to_string(),
                index: item.naming.index,
                index_pad: item.naming.index_pad,
                tz_offset_min: item.naming.tz_offset_min,
                quality: quality.map(quality_name).unwrap_or_default().to_string(),
                codec: codec_label.to_string(),
                date: date.clone().unwrap_or_default(),
                publish_date: item.naming.publish_date.clone(),
            };
            // 预告的名字要和真正落盘的一致：先拼文件夹层级，再按来源类型定文件名/文件夹
            let leaf = match item.kind.as_str() {
                "audio" => crate::naming::render(&settings.naming_template, &ctx, "m4a"),
                "opus" | "article" => crate::naming::render_dir(&settings.naming_template, &ctx),
                _ => crate::naming::render(&settings.naming_template, &ctx, &ext),
            };
            settings
                .output_folder_template(&ctx)
                .join(leaf)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    Ok(names)
}

/// 「魔法变量」面板的数据源：界面直接渲染这份清单，不再手写第二份可能和后端脱节的表。
/// 栏目 + 短标签 + 悬停说明一起给出去，面板按栏目分列横排。
#[tauri::command]
pub async fn naming_variables() -> Result<Vec<crate::types::NamingVariable>, String> {
    Ok(crate::naming::VARIABLES
        .iter()
        .map(|(token, label, section, hint)| crate::types::NamingVariable {
            token: (*token).to_string(),
            label: (*label).to_string(),
            section: (*section).to_string(),
            hint: (*hint).to_string(),
        })
        .collect())
}

/// 内容库的一行：账号里的一个收藏夹（或订阅的）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct FavFolder {
    /// 收藏夹 id：打开它要拼 `space.bilibili.com/<mid>/favlist?fid=<id>`
    pub id: i64,
    pub title: String,
    pub media_count: u64,
    /// 订阅来的那些，这里是被订阅收藏夹的作者
    #[serde(default)]
    pub owner: String,
    /// 封面（只有订阅的接口给；我创建的收藏夹没有封面）
    #[serde(default)]
    pub cover: String,
    /// 简介（同样只有订阅的接口给）
    #[serde(default)]
    pub intro: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FavFolders {
    pub mid: u64,
    pub created: Vec<FavFolder>,
    pub subscribed: Vec<FavFolder>,
}

/// 内容库：读**账号里的**收藏夹与订阅（不是用户手填的清单）。
///
/// 两个接口都吃登录态：`created/list-all` 给"我创建的"，`collected/list` 给"我订阅的"。
/// 没登录时两个都拿不到，前端会显示"登录后连接你的内容库"。
#[tauri::command]
pub async fn library_folders(state: State<'_, AppState>) -> Result<FavFolders, String> {
    let client = state.client();
    let login = current_login(&client).await;
    let mid = login.mid;
    if mid == 0 {
        return Err("未登录：登录后才能读账号里的收藏夹与订阅".to_string());
    }

    // 我创建的（含默认收藏夹）
    let created: serde_json::Value = client
        .fetch_json(&format!(
            "https://api.bilibili.com/x/v3/fav/folder/created/list-all?up_mid={mid}"
        ))
        .await
        .map_err(describe)?;
    // 我订阅的（别人公开的收藏夹/合集）
    let collected: serde_json::Value = client
        .fetch_json(&format!(
            "https://api.bilibili.com/x/v3/fav/folder/collected/list?up_mid={mid}&pn=1&ps=50&platform=web"
        ))
        .await
        .map_err(describe)?;

    let parse = |value: &serde_json::Value| -> Vec<FavFolder> {
        value
            .get("list")
            .and_then(|list| list.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|item| {
                        let id = item.get("id")?.as_i64()?;
                        let title = item
                            .get("title")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let media_count = item
                            .get("media_count")
                            .and_then(|c| c.as_u64())
                            .unwrap_or_default();
                        let owner = item
                            .get("upper")
                            .and_then(|u| u.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or_default()
                            .to_string();
                        // 收藏夹自己的封面字段叫 cover，订阅接口还给 intro
                        let cover = item
                            .get("cover")
                            .and_then(|c| c.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let intro = item
                            .get("intro")
                            .and_then(|c| c.as_str())
                            .unwrap_or_default()
                            .to_string();
                        Some(FavFolder {
                            id,
                            title,
                            media_count,
                            owner,
                            cover,
                            intro,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    Ok(FavFolders {
        mid,
        created: parse(&created),
        subscribed: parse(&collected),
    })
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
    // true = 按「文件夹层级」渲染（不补扩展名、丢掉空壳段落）
    dir: Option<bool>,
) -> Result<String, String> {
    let mut ctx = crate::naming::NamingContext::sample();
    ctx.date = date.unwrap_or_default();
    ctx.publish_date = publish_date.unwrap_or_default();
    let ext = ext.unwrap_or_else(|| state.settings().container_ext().to_string());
    let path = if dir.unwrap_or(false) {
        crate::naming::render_dir(&template, &ctx)
    } else {
        crate::naming::render(&template, &ctx, &ext)
    };
    // 用 / 显示，和用户在模板里打的保持一致（Windows 的 PathBuf 会显示成 \）
    Ok(path.to_string_lossy().replace('\\', "/"))
}

/// 单次加载上限，避免超大来源被一次拉上千条（界面上可以「继续解析」分批拉）。
const FAV_MAX_ITEMS: usize = 500;
const COLLECTION_MAX_ITEMS: usize = 500;
const SPACE_MAX_ITEMS: usize = 300;
/// 图文要拉完整份列表才能算出"第几条"（接口不给总数），上限必须够宽
const OPUS_MAX_ITEMS: usize = 2000;

/// 各来源的单页条数，决定「继续解析」一次往后拉多少页。
fn source_page_size(target: BatchTarget) -> usize {
    match target {
        BatchTarget::Fav(_) | BatchTarget::Whole => 20,
        BatchTarget::Space(_) => 30,
        // 系列接口一页最多 30 条（合集能给 100）
        BatchTarget::Series { .. } => 30,
        // 图文按 offset 游标翻页，这里只用于"首页取多少"的判定
        BatchTarget::Opus(_) => 20,
        // 音频接口一页最多 30 条
        BatchTarget::Audio(_) => 30,
        BatchTarget::Collection { .. } => 100,
    }
}

/// 单次解析上限：设置里填了就用它，否则按来源类型给默认值。
fn source_cap(kind: &str, override_cap: usize) -> usize {
    if override_cap > 0 {
        return override_cap;
    }
    match kind {
        "fav" => FAV_MAX_ITEMS,
        "collection" | "series" => COLLECTION_MAX_ITEMS,
        // 图文：拉完整份列表才算得出编号，用单独的宽上限
        "opus" => OPUS_MAX_ITEMS,
        "audio" => SPACE_MAX_ITEMS,
        _ => SPACE_MAX_ITEMS,
    }
}

/// 按序号加载：算出要拉的页码与页内偏移（序号从 1 起）。
///
/// 例：UP 空间每页 30 条，从第 301 条开始 → 第 11 页、页内偏移 0；
/// 从第 305 条开始 → 第 11 页、偏移 4。
fn range_slice(from: usize, page_size: usize) -> (u32, usize) {
    let from = from.max(1);
    let page_size = page_size.max(1);
    (
        ((from - 1) / page_size) as u32 + 1,
        (from - 1) % page_size,
    )
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

/// 这一页之后还有没有可拉的了。
///
/// - 番剧/课程一次给全，没有下一页
/// - 分页来源：已经到总数，或者这一页没取满（说明是最后一页）
fn batch_exhausted(target: BatchTarget, loaded: usize, total: usize) -> bool {
    if matches!(target, BatchTarget::Whole) {
        return true;
    }
    loaded >= total || source_page_size(target) > loaded
}

/// 图文的"标题"：取正文摘要（列表与命名都用它）。正文为空时退回 id。
fn summary_of(content: &str, opus_id: &str) -> String {
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = flat.trim();
    if trimmed.is_empty() {
        return format!("图文 {opus_id}");
    }
    let mut out: String = trimmed.chars().take(60).collect();
    if trimmed.chars().count() > 60 {
        out.push('…');
    }
    out
}

/// 条目自己没有上传者时用来源的补上（合集接口不给每条的上传者）。
fn fill_missing_owner(items: &mut [BatchVideo], owner: &str) {
    if owner.is_empty() {
        return;
    }
    for item in items.iter_mut() {
        if item.owner.is_empty() {
            item.owner = owner.to_string();
        }
    }
}

/// 第一页返回的来源信息（后续页不再重复给）。
struct BatchMeta {
    kind: String,
    title: String,
    owner: String,
    total: usize,
    /// 合集所属 UP 的 mid（只有合集接口会给，用来识别"系列 id 当合集查"的错配）
    mid: u64,
    /// 这一页之后还有没有内容（图文接口直接给 has_more；分页接口按页数自己判断）
    has_more: bool,
    /// 图文列表的下一页游标（其他来源为空）
    next_offset: String,
}

/// 拉批量来源的一页。`meta` 只有第一页有值。
async fn fetch_batch_page(
    client: &BiliClient,
    target: BatchTarget,
    page: u32,
    offset: &str,
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
                mid: 0,
                has_more: true,
                next_offset: String::new(),
            });
            let items = data
                .medias
                .iter()
                .map(|media| BatchVideo {
                    bvid: media.bvid.clone(),
                    cid: media.cid,
                    ep_id: None,
                    opus_id: String::new(),
                    au_id: String::new(),
                    collection: String::new(),
                    title: media.title.clone(),
                    owner: media.upper.name.clone(),
                    pic: media.cover.clone(),
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
            // 合集条目里没有上传者，只有 meta.mid，第一页顺带查一次 UP 名字
            let owner = if first && data.meta.mid > 0 {
                client.user_name(data.meta.mid).await
            } else {
                String::new()
            };
            let meta = first.then(|| BatchMeta {
                kind: "collection".to_string(),
                title: data.meta.name.clone(),
                owner,
                total: data.meta.total as usize,
                mid: data.meta.mid,
                has_more: true,
                next_offset: String::new(),
            });
            let items = data
                .archives
                .iter()
                .map(|archive| BatchVideo {
                    bvid: archive.bvid.clone(),
                    cid: archive.cid,
                    ep_id: None,
                    opus_id: String::new(),
                    au_id: String::new(),
                    collection: String::new(),
                    title: archive.title.clone(),
                    owner: archive.owner.name.clone(),
                    pic: archive.pic.clone(),
                    duration: archive.duration,
                })
                .collect();
            Ok((items, meta))
        }
        BatchTarget::Space(mid) => {
            let data = client.space_archives(mid, page).await.map_err(describe)?;
            let list = data.list.as_ref();
            // 列表里的 author 偶尔是空的（风控页、合作稿件等），那就按 mid 反查一次，
            // 否则来源标题会变成没有名字的" 的投稿"，子文件夹也跟着变空。
            let mut owner = list
                .and_then(|l| l.vlist.first())
                .map(|v| v.author.clone())
                .unwrap_or_default();
            if owner.trim().is_empty() && first {
                owner = client.user_name(mid).await;
            }
            let meta = first.then(|| BatchMeta {
                kind: "space".to_string(),
                title: if owner.trim().is_empty() {
                    "UP 投稿".to_string()
                } else {
                    format!("{owner} 的投稿")
                },
                owner,
                total: data.page.count as usize,
                mid: 0,
                has_more: true,
                next_offset: String::new(),
            });
            let items = list
                .map(|l| {
                    l.vlist
                        .iter()
                        .map(|video| BatchVideo {
                            bvid: video.bvid.clone(),
                            cid: 0,
                            ep_id: None,
                            opus_id: String::new(),
                            au_id: String::new(),
                            collection: String::new(),
                            title: video.title.clone(),
                            owner: video.author.clone(),
                            pic: video.pic.clone(),
                            duration: parse_mmss(&video.length),
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok((items, meta))
        }
        // 系列：接口不给标题也不给上传者，第一页顺带各查一次
        BatchTarget::Series { mid, sid } => {
            let data = client.series_archives(mid, sid, page).await.map_err(describe)?;
            let owner = if first {
                client.user_name(mid).await
            } else {
                String::new()
            };
            // 标题要额外请求一次，先 await 出结果再构造 meta
            let title = if first {
                let name = client
                    .series_meta(mid, sid)
                    .await
                    .map(|page| page.meta.name)
                    .unwrap_or_default();
                if name.trim().is_empty() {
                    "系列".to_string()
                } else {
                    name
                }
            } else {
                String::new()
            };
            let meta = first.then(|| BatchMeta {
                kind: "series".to_string(),
                title,
                // owner 随后还要逐条填进条目里，这里先克隆一份
                owner: owner.clone(),
                total: data.page.total as usize,
                mid: 0,
                has_more: true,
                next_offset: String::new(),
            });
            let items = data
                .archives
                .iter()
                .map(|archive| BatchVideo {
                    bvid: archive.bvid.clone(),
                    // 系列条目没有 cid，下载与画质探测会按 bvid 补查
                    cid: 0,
                    ep_id: None,
                    opus_id: String::new(),
                    au_id: String::new(),
                    collection: String::new(),
                    title: archive.title.clone(),
                    owner: owner.clone(),
                    pic: archive.pic.clone(),
                    duration: archive.duration,
                })
                .collect();
            Ok((items, meta))
        }
        // 图文：20 条一页，靠 offset 游标往后翻（page 参数无效）
        BatchTarget::Opus(mid) => {
            let data = client.opus_feed(mid, offset).await.map_err(describe)?;
            let owner = if first {
                client.user_name(mid).await
            } else {
                String::new()
            };
            let items = data
                .items
                .iter()
                .map(|item| BatchVideo {
                    bvid: String::new(),
                    cid: 0,
                    ep_id: None,
                    opus_id: item.opus_id.clone(),
                    au_id: String::new(),
                    collection: String::new(),
                    title: summary_of(&item.content, &item.opus_id),
                    owner: owner.clone(),
                    pic: item
                        .cover
                        .as_ref()
                        .map(|c| c.url.clone())
                        .unwrap_or_default(),
                    duration: 0,
                })
                .collect();
            // 图文每页都带 meta：extend_batch 要靠 has_more 与游标继续翻
            let meta = Some(BatchMeta {
                kind: "opus".to_string(),
                title: if first {
                    format!("{} 的图文", if owner.is_empty() { "该 UP" } else { &owner })
                } else {
                    String::new()
                },
                owner,
                // 图文接口不给总数，界面按"已加载 N 项"显示
                total: 0,
                mid: 0,
                has_more: data.has_more,
                next_offset: data.offset.clone(),
            });
            Ok((items, meta))
        }
        // 音频：30 条一页，常规页码翻页
        BatchTarget::Audio(mid) => {
            let data = client.audio_list(mid, page).await.map_err(describe)?;
            let owner = data
                .items
                .first()
                .map(|item| item.uname.clone())
                .unwrap_or_default();
            let meta = first.then(|| BatchMeta {
                kind: "audio".to_string(),
                title: format!(
                    "{} 的音频",
                    if owner.is_empty() { "该 UP" } else { &owner }
                ),
                owner,
                total: data.total_size as usize,
                mid: 0,
                has_more: true,
                next_offset: String::new(),
            });
            let items = data
                .items
                .iter()
                .map(|item| BatchVideo {
                    bvid: String::new(),
                    cid: 0,
                    ep_id: None,
                    opus_id: String::new(),
                    au_id: item.id.to_string(),
                    collection: String::new(),
                    title: item.title.clone(),
                    owner: if item.uname.is_empty() {
                        item.author.clone()
                    } else {
                        item.uname.clone()
                    },
                    pic: String::new(),
                    duration: item.duration,
                })
                .collect();
            Ok((items, meta))
        }
        // 番剧/课程一次给全，没有分页
        BatchTarget::Whole => Ok((Vec::new(), None)),
    }
}

/// 解析批量来源：拉第一页建缓存，顺带探一次可用清晰度/音轨。
///
/// `cap` 是这一批的单次上限快照，之后改设置不影响已经打开的清单。
async fn start_batch(
    client: &BiliClient,
    target: BatchTarget,
    meta: BatchMeta,
    items: Vec<BatchVideo>,
    cap: usize,
) -> Result<BatchCache, String> {
    if items.is_empty() {
        return Err(match meta.kind.as_str() {
            "fav" => "收藏夹为空或不可访问".to_string(),
            "collection" => "合集为空或不可访问".to_string(),
            "series" => "系列为空或不可访问".to_string(),
            "audio" => "这个 UP 没有音频投稿（B 站音频接口在缺 order/platform 参数时也会返回空）".to_string(),
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

    let mut items = items;
    fill_missing_owner(&mut items, &meta.owner);

    let exhausted = if meta.kind == "opus" {
        !meta.has_more
    } else {
        batch_exhausted(target, items.len(), meta.total)
    };
    Ok(BatchCache {
        kind: meta.kind,
        target,
        title: meta.title,
        owner: meta.owner,
        total: meta.total,
        items,
        from_index: 1,
        cap,
        next_page: 2,
        next_offset: meta.next_offset.clone(),
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
    let cap = cache.cap;
    let before = cache.items.len();
    while cache.items.len() - before < want && !cache.exhausted {
        if cache.items.len() >= cap {
            cache.exhausted = true;
            break;
        }
        let page = cache.next_page;
        let (items, meta) =
            fetch_batch_page(client, cache.target, page, &cache.next_offset).await?;
        if items.is_empty() {
            cache.exhausted = true;
            break;
        }
        let mut items = items;
        fill_missing_owner(&mut items, &cache.owner);
        cache.items.extend(items);
        cache.next_page = page + 1;
        if let Some(meta) = &meta {
            if cache.kind == "opus" {
                // 图文没有页码：靠接口给的 has_more 与下一页游标推进
                cache.exhausted = !meta.has_more;
                cache.next_offset = meta.next_offset.clone();
            }
        }
        // total=0 表示"总数未知"（图文），不能拿它判到底
        if cache.total > 0 && cache.items.len() >= cache.total {
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
        "series" => "系列",
        "opus" => "图文",
        "audio" => "音频",
        "space" => "投稿",
        _ => "来源",
    };
    ProbeSource {
        kind: cache.kind.clone(),
        key: String::new(),
        title: cache.title.clone(),
        owner: cache.owner.clone(),
        cover: String::new(),
        note: if cache.kind == "opus" {
            // 图文接口不给总数，只能提示上限
            if loaded >= cache.cap {
                format!("图文已达单次解析上限 {} 条（可在设置里调大）", cache.cap)
            } else {
                String::new()
            }
        } else {
            load_note(label, cache.total, loaded, cache.cap, cache.exhausted)
        },
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
        from_index: cache.from_index,
        loaded,
        exhausted: cache.exhausted,
        capped: loaded >= cache.cap,
        qualities: cache.qualities.clone(),
        audios: cache.audios.clone(),
        recommended_quality: cache.recommended_quality,
        best_quality: cache.best_quality,
        items: cache.items.clone(),
    }
}

/// 来源身份：同一个合集的不同视频展开后 key 相同，前端据此判重，
/// 避免同一合集被解析两遍、同一条内容在列表里出现两次。
fn target_key(target: &Target) -> String {
    match target {
        Target::Bvid(bvid) => format!("video:{bvid}"),
        Target::Aid(aid) => format!("aid:{aid}"),
        Target::FavList(fid) => format!("fav:{fid}"),
        Target::Collection { mid, sid } => format!("collection:{mid}:{sid}"),
        Target::Series { mid, sid } => format!("series:{mid}:{sid}"),
        Target::OpusList(mid) => format!("opus:{mid}"),
        Target::AudioList(mid) => format!("audio:{mid}"),
        Target::Post { id, article } => {
            format!("{}:{id}", if *article { "article" } else { "post" })
        }
        Target::Space(mid) => format!("space:{mid}"),
        Target::Bangumi { season_id, ep_id } => match (season_id, ep_id) {
            (Some(sid), _) => format!("bangumi:{sid}"),
            (_, Some(ep)) => format!("bangumi:ep:{ep}"),
            _ => "bangumi:unknown".to_string(),
        },
        Target::Cheese(season_id) => format!("cheese:{season_id}"),
    }
}

/// 解析一条来源：短链展开 → 识别目标 → 单视频取详情，批量来源拉第一页。
///
/// 批量来源的加载进度存进 AppState，供「继续解析」接着往后拉，不必从第一页重来。
async fn probe_one(
    client: &BiliClient,
    state: &AppState,
    input: &str,
    prefer_collection: bool,
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
    // 来源身份：要在 match 吃掉 target 之前算好
    let key = target_key(&target);

    match target {
        Target::Bvid(bvid) => {
            probe_video_or_collection(client, state, trimmed, &bvid, prefer_collection).await
        }
        Target::Aid(aid) => {
            let info = client.video_info_by_aid(aid).await.map_err(describe)?;
            probe_video_or_collection(client, state, trimmed, &info.bvid, prefer_collection).await
        }

        // ---------- 批量来源：只拉第一页 ----------
        Target::FavList(fid) => {
            probe_batch_first_page(client, state, trimmed, BatchTarget::Fav(fid)).await
        }
        Target::Collection { mid, sid } => {
            probe_batch_first_page(client, state, trimmed, BatchTarget::Collection { mid, sid }).await
        }
        Target::Series { mid, sid } => {
            probe_batch_first_page(client, state, trimmed, BatchTarget::Series { mid, sid }).await
        }
        Target::OpusList(mid) => {
            probe_batch_first_page(client, state, trimmed, BatchTarget::Opus(mid)).await
        }
        Target::AudioList(mid) => {
            probe_batch_first_page(client, state, trimmed, BatchTarget::Audio(mid)).await
        }
        Target::Post { id, article } => probe_post(client, trimmed, id, article).await,
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
    .map(|mut source| {
        // 视频展开成合集时，函数内部已经把 key 设成合集身份，这里不要覆盖
        if source.key.is_empty() {
            source.key = key;
        }
        source
    })
}

/// 单个视频链接：批量解析模式下先看它属于哪个合集，属于就把整个合集拉出来；
/// 单个视频模式（或不在合集里）就只解析这一个。
async fn probe_video_or_collection(
    client: &BiliClient,
    state: &AppState,
    key: &str,
    bvid: &str,
    prefer_collection: bool,
) -> Result<ProbeSource, String> {
    if prefer_collection {
        if let Ok(info) = client.video_info(bvid).await {
            let season = info.ugc_season.filter(|s| s.id > 0 && s.mid > 0);
            if let Some(season) = season {
                let target = BatchTarget::Collection {
                    mid: season.mid,
                    sid: season.id,
                };
                let (items, meta) = fetch_batch_page(client, target, 1, "").await?;
                if let Some(meta) = meta {
                    let mut source = finish_batch(client, state, key, target, meta, items).await?;
                    // 身份要记成合集：同一个合集里的另一个视频也会展开成这个合集，
                    // 记成 video:BV... 的话判重就抓不到
                    source.key = format!("collection:{}:{}", season.mid, season.id);
                    source.note = format!("该视频属于合集「{}」，已按合集解析", season.title);
                    return Ok(source);
                }
            }
        }
    }
    probe_video_bvid(client, bvid).await
}

/// 单条图文/专栏：抓一次页面状态，包装成"只有一条"的来源。
///
/// 专栏的旧链接会 301 到 opus 页，两种形态拿到的是同一份页面状态，
/// 下载也走图文那条路（图片 + 正文存文件夹）。
async fn probe_post(
    client: &BiliClient,
    key: &str,
    id: u64,
    article: bool,
) -> Result<ProbeSource, String> {
    let html = client.post_page(id, article).await.map_err(describe)?;
    let post = bili_core::opus::parse_page(&html).map_err(describe)?;
    let id_text = id.to_string();
    let title = if post.title.trim().is_empty() {
        summary_of(&post.text, &id_text)
    } else {
        post.title.clone()
    };
    Ok(ProbeSource {
        kind: if article { "article" } else { "opus" }.to_string(),
        key: key.trim().to_string(),
        title,
        owner: post.author.clone(),
        cover: String::new(),
        note: String::new(),
        bvid: String::new(),
        cid: 0,
        aid: 0,
        owner_mid: 0,
        pubdate: 0,
        part_index: 0,
        part_title: String::new(),
        duration: 0,
        page_count: 1,
        total: 1,
        from_index: 1,
        loaded: 1,
        exhausted: true,
        capped: false,
        qualities: Vec::new(),
        audios: Vec::new(),
        recommended_quality: 0,
        best_quality: 0,
        items: vec![BatchVideo {
            bvid: String::new(),
            cid: 0,
            ep_id: None,
            opus_id: id_text,
            au_id: String::new(),
            collection: String::new(),
            title: post.title.clone(),
            owner: post.author,
            pic: String::new(),
            duration: 0,
        }],
    })
}

/// 分页类批量来源：拉第一页 → 建缓存 → 返回。
async fn probe_batch_first_page(
    client: &BiliClient,
    state: &AppState,
    key: &str,
    target: BatchTarget,
) -> Result<ProbeSource, String> {
    let (items, meta) = fetch_batch_page(client, target, 1, "").await?;
    let meta = meta.ok_or_else(|| "来源没有可访问的内容".to_string())?;

    // 系列和合集的 id 空间重叠：拿系列 id 去问合集接口，会返回**别人的**合集
    // （code 正常、内容却是另一回事，见 90946 那次踩坑）。合集接口给的 mid
    // 对不上这个 UP 时，就按系列重取一次。
    if let BatchTarget::Collection { mid, sid } = target {
        if meta.mid != 0 && meta.mid != mid {
            let series = BatchTarget::Series { mid, sid };
            let (items, meta) = fetch_batch_page(client, series, 1, "").await.map_err(|e| {
                format!("这个链接既不是该 UP 的合集，也不是系列：{e}")
            })?;
            let meta = meta.ok_or_else(|| "来源没有可访问的内容".to_string())?;
            return finish_batch(client, state, key, series, meta, items).await;
        }
    }

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
    let cap = source_cap(&meta.kind, state.settings().parse_cap);
    let cache = start_batch(client, target, meta, items, cap).await?;
    let source = batch_to_source(&cache);
    state.put_batch(key.to_string(), cache);
    Ok(source)
}

/// 按序号加载：把这个来源的第 `from` 条起的一批（到单次上限为止）取出来。
///
/// 超过单次上限的来源用它可以分几次拉完，而且两批互不重叠：
/// 1337 条投稿、上限 300 时，第一次 1–300、第二次 301–600……
/// 配合默认的「重名跳过」策略，重复下载也不会产生副本。
async fn probe_range_one(
    client: &BiliClient,
    state: &AppState,
    input: &str,
    from: usize,
) -> Result<ProbeSource, String> {
    let key = input.trim();
    if key.is_empty() {
        return Err("来源为空".to_string());
    }

    let resolved = if is_short_link(key) {
        client.resolve_redirect(key).await.map_err(describe)?
    } else {
        key.to_string()
    };
    let target = parse_target(&resolved).map_err(describe)?;
    // 只有分页类来源能按序号取；番剧/课程一次给全，用不着分批
    let target = match target {
        Target::FavList(fid) => BatchTarget::Fav(fid),
        Target::Collection { mid, sid } => BatchTarget::Collection { mid, sid },
        Target::Space(mid) => BatchTarget::Space(mid),
        Target::AudioList(mid) => BatchTarget::Audio(mid),
        _ => return Err("这个来源一次就能全部拿到，不需要按序号分批解析".to_string()),
    };

    let from = from.max(1);
    let (page, skip) = range_slice(from, source_page_size(target));

    // 标题与总数只在第一页给：优先复用同一来源已经解析出来的那份
    let cached_meta = state.peek_batch(key).map(|cache| BatchMeta {
        kind: cache.kind,
        title: cache.title,
        owner: cache.owner,
        total: cache.total,
        mid: 0,
        has_more: !cache.exhausted,
        next_offset: String::new(),
    });
    let (mut items, meta) = fetch_batch_page(client, target, page, "").await?;
    let meta = match meta.or(cached_meta) {
        Some(meta) => meta,
        None => {
            let (_, meta) = fetch_batch_page(client, target, 1, "").await?;
            meta.ok_or_else(|| "来源没有可访问的内容".to_string())?
        }
    };

    if from > meta.total {
        return Err(format!("这个来源只有 {} 条，第 {from} 条不存在", meta.total));
    }
    items.drain(..skip.min(items.len()));
    if items.is_empty() {
        return Err(format!("第 {from} 条之后没有可下载的内容了"));
    }

    let cap = source_cap(&meta.kind, state.settings().parse_cap);
    let mut cache = start_batch(client, target, meta, items, cap).await?;
    cache.from_index = from;
    cache.next_page = page + 1;
    // 一次就拉到这一批的上限：点一下能拿到"第 301–600 条"这样的整批。
    // 中途失败不算错——已经拿到的条目照样可用，「继续解析」可以接着拉。
    let _ = extend_batch(client, &mut cache, cap).await;
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
        capped: cache.items.len() >= cache.cap,
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
                    opus_id: String::new(),
                    au_id: String::new(),
                    collection: String::new(),
                    title: if episode.long_title.is_empty() {
                        episode.title.clone()
                    } else {
                        format!("{} {}", episode.title, episode.long_title)
                    },
                    // 番剧每集没有上传者，用出品方（如"哔哩哔哩番剧"）
                    owner: season.up_info.uname.clone(),
                    pic: String::new(),
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
                    opus_id: String::new(),
                    au_id: String::new(),
                    collection: String::new(),
                    title: episode.title.clone(),
                    owner: season.up_info.uname.clone(),
                    // 课程接口没给封面，留空（卡片上显示占位）
                    pic: String::new(),
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
            mid: 0,
            has_more: true,
            next_offset: String::new(),
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
        key: String::new(),
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
        from_index: 1,
        loaded: 1,
        exhausted: true,
        capped: false,
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

/// 来源类型的中文名：文件夹分层模板里用 `{source_kind}` 时取这个值。
fn kind_label(kind: &str) -> &'static str {
    match kind {
        "fav" => "收藏夹",
        "collection" => "合集",
        "series" => "系列",
        "opus" => "图文",
        "article" => "专栏",
        "audio" => "音频",
        "space" => "UP 空间",
        "bangumi" => "番剧",
        "cheese" => "课程",
        _ => "视频",
    }
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
        source_kind: kind_label(&req.source).to_string(),
        index: req.naming.index,
        index_pad: req.naming.index_pad,
        tz_offset_min: req.naming.tz_offset_min,
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
    Ok(enqueue_download(&app, &state, req))
}

/// 把一条下载请求放进队列（`start_download` 与启动续传共用）。
fn enqueue_download(app: &AppHandle, state: &AppState, req: DownloadRequest) -> String {
    let id = state.next_task_id();
    let mut initial = TaskUpdate::new(id.clone(), &req);
    initial.quality_label = if req.source == "opus" {
        "图文".to_string()
    } else {
        quality_name(req.quality).to_string()
    };
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
        // 播放地址会自动过期：开着「链接过期时自动刷新」时，失败就重取地址再来一次。
        // 分片记录（*.ranges）让已经下过的字节不重下，所以这个重试很便宜。
        let mut attempt = 0u32;
        loop {
            let result = run_download(
                task_app.clone(),
                client.clone(),
                slots.clone(),
                settings.clone(),
                output_dir.clone(),
                &req,
                shared_for_task.clone(),
            )
            .await;
            match result {
                Ok(()) => break,
                Err(e) => {
                    let expired = e.to_string().contains("403") || e.to_string().contains("410");
                    if expired && settings.auto_refresh_urls && attempt < 2 {
                        attempt += 1;
                        settings.log(
                            "warn",
                            &format!("播放地址可能过期，重取地址后继续（第 {attempt} 次）: {e}"),
                        );
                        continue;
                    }
                    mutate(&shared_for_task, &task_app, |t| {
                        if t.status != TaskStatus::Done && t.status != TaskStatus::Canceled {
                            t.status = TaskStatus::Failed;
                            t.message = e.to_string();
                        }
                    });
                    break;
                }
            }
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
    id
}

/// 启动续传：把 `.bilitmp` 里没下完的任务重新入队。
///
/// 每个未完成的下载目录里有一份 `task.json`（原始请求）；重新入队会重新取播放地址，
/// 分片记录（`*.ranges`）让已经下过的字节不再重下 —— 这就是"断点续传 + 播放地址自动刷新"。
#[tauri::command]
pub async fn resume_pending(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    if !state.settings().resume_on_start {
        return Ok(0);
    }
    let dir = state.output_dir().join(".bilitmp");
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
        return Ok(0);
    };
    let mut started = 0usize;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let task_file = entry.path().join("task.json");
        let Ok(text) = tokio::fs::read_to_string(&task_file).await else {
            continue;
        };
        let Ok(req) = serde_json::from_str::<DownloadRequest>(&text) else {
            continue;
        };
        state
            .settings()
            .log("info", &format!("续传未完成任务: {}", req.title));
        enqueue_download(&app, &state, req);
        started += 1;
    }
    Ok(started)
}

/// 图文下载：一个条目一个文件夹，里面是原图（按序号）与正文 txt。
///
/// 列表接口只给摘要与封面，完整的正文和原图要从 opus 页面的内嵌状态里解。
async fn run_opus_download(
    app: AppHandle,
    client: Arc<BiliClient>,
    settings: crate::state::Settings,
    output_dir: PathBuf,
    req: &DownloadRequest,
    shared: Arc<Mutex<TaskUpdate>>,
) -> Result<(), BiliError> {
    if req.opus_id.is_empty() {
        return Err(BiliError::InvalidInput("缺少图文 id".into()));
    }
    mutate(&shared, &app, |t| {
        t.message = "获取图文内容".to_string();
    });

    // 专栏走旧链接（会 301 到 opus 页），图文走 opus 直链
    let html = client
        .post_page(
            req.opus_id
                .parse()
                .map_err(|_| BiliError::InvalidInput("图文 id 非法".into()))?,
            req.source == "article",
        )
        .await?;
    let post = bili_core::opus::parse_page(&html)?;

    // 编号由前端统一给出（图文用"来源内固定位置"，冻结不漂移），
    // 这里不再用发布日期覆盖 —— 预览与落盘必须是同一个值。
    let naming = naming_context(req, "", "图文");
    let folder = output_dir
        .join(settings.output_folder_template(&naming))
        .join(settings.output_folder(&naming));
    let text_path = folder.join("正文.txt");

    // 重名策略与视频一致：跳过 / 自动加序号 / 覆盖
    let folder = match settings.rename_conflict.as_str() {
        "overwrite" => folder,
        "auto" => find_free_name(folder).await,
        _ => {
            if text_path.exists() {
                let path = folder.to_string_lossy().to_string();
                settings.log("info", &format!("图文已存在，跳过任务: {path}"));
                mutate(&shared, &app, |t| {
                    t.status = TaskStatus::Done;
                    t.video_pct = 100.0;
                    t.audio_pct = 100.0;
                    t.output_path = path.clone();
                    t.message = "文件夹已存在，跳过下载".to_string();
                });
                return Ok(());
            }
            folder
        }
    };
    let text_path = folder.join("正文.txt");
    tokio::fs::create_dir_all(&folder).await?;

    let total = post.images.len();
    settings.log(
        "info",
        &format!("图文 {}：{} 张图，{} 字", req.opus_id, total, post.text.chars().count()),
    );
    tokio::fs::write(&text_path, compose_opus_text(&post, &req.opus_id)).await?;

    // 图片格式：默认原样落盘；选了 JPG 就把不是 jpg 的转一道（转不了就留原格式）
    let want_jpg = settings.image_format == "jpg";
    let ffmpeg_bin = bili_core::ffmpeg::find_ffmpeg(explicit_ffmpeg(&settings).as_deref());
    if want_jpg && ffmpeg_bin.is_none() {
        settings.log("warn", "选了 JPG 图片但没找到 ffmpeg，图片保持原格式");
    }
    for (index, image) in post.images.iter().enumerate() {
        let bytes = client.fetch_bytes(&image.url).await?;
        let source_ext = image_ext(&image.url, &bytes.1);
        let mut jpeg: Option<Vec<u8>> = None;
        if want_jpg && source_ext != "jpg" {
            if let Some(ffmpeg) = ffmpeg_bin.as_deref() {
                match jpeg_bytes(ffmpeg, &bytes.0, source_ext, &folder, index).await {
                    Ok(data) => jpeg = Some(data),
                    Err(e) => settings.log("warn", &format!("转 JPG 失败，保留原格式: {e}")),
                }
            }
        }
        let ext = if jpeg.is_some() { "jpg" } else { source_ext };
        let data = jpeg.as_deref().unwrap_or(&bytes.0);
        tokio::fs::write(folder.join(format!("{:02}.{ext}", index + 1)), data).await?;
        let done = index + 1;
        let percent = done as f64 / total.max(1) as f64 * 100.0;
        mutate(&shared, &app, |t| {
            t.video_pct = percent;
            t.audio_pct = 100.0;
            t.message = format!("图片 {done}/{total}");
        });
    }

    mutate(&shared, &app, |t| {
        t.status = TaskStatus::Done;
        t.video_pct = 100.0;
        t.audio_pct = 100.0;
        t.output_path = folder.to_string_lossy().to_string();
        t.message = if total == 0 {
            "已保存正文（这条没有图片）".to_string()
        } else {
            format!("已保存 {total} 张图片与正文")
        };
    });
    Ok(())
}

/// 音频下载：取播放地址，直接把 m4a 落盘（不需要合并，也没有视频流）。
async fn run_audio_download(
    app: AppHandle,
    client: Arc<BiliClient>,
    settings: crate::state::Settings,
    output_dir: PathBuf,
    req: &DownloadRequest,
    shared: Arc<Mutex<TaskUpdate>>,
) -> Result<(), BiliError> {
    let sid: u64 = req
        .au_id
        .parse()
        .map_err(|_| BiliError::InvalidInput("音频 id 非法".into()))?;
    mutate(&shared, &app, |t| {
        t.message = "获取音频地址".to_string();
    });

    let stream = client.audio_stream(sid).await?;
    let url = stream
        .cdns
        .first()
        .cloned()
        .ok_or_else(|| BiliError::Unavailable("这条音频没有可下载的地址（可能是会员专享）".into()))?;

    // 音频默认用原轨道的扩展名（B 站给的基本是 m4a）；设置里选了 MP3 就转码成 mp3
    let source_ext = audio_ext(&url);
    let want_mp3 = settings.audio_format == "mp3";
    let target_ext = if want_mp3 { "mp3" } else { source_ext };
    let naming = naming_context(req, "", "音频");
    let out_file = output_dir
        .join(settings.output_folder_template(&naming))
        .join(settings.output_filename_with_ext(&naming, target_ext));
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

    let opts = DownloadOptions {
        concurrency: settings.chunk_concurrency,
        chunk_size: settings.chunk_mb * 1024 * 1024,
        retries: settings.retry_count as usize,
        speed_limit_bps: settings.speed_limit_mib as u64 * 1024 * 1024,
        // 音频来源是小文件、直接写成品，不做分片续传
        resume_log: None,
    };
    let throttle = Throttle::new(opts.speed_limit_bps);
    let final_path = out_file.clone();
    {
        let shared = shared.clone();
        let app = app.clone();
        let on_progress: ProgressFn = Arc::new(move |p: Progress| {
            mutate(&shared, &app, |t| {
                t.status = TaskStatus::Downloading;
                t.message = "下载音频".to_string();
                t.video_pct = percent(p.downloaded, p.total);
                t.video_bytes = p.downloaded;
                t.video_total = p.total;
                t.speed_bps = p.speed_bps;
                t.recalc();
            });
        });
        download_with_throttle(&client.http, &url, &[], &out_file, &opts, on_progress, throttle.as_ref())
            .await?;
    }

    // 要 MP3 而源不是 mp3（常见是 m4a）时转一道；转不了就留着原格式，别让任务失败
    if want_mp3 && source_ext != "mp3" {
        match bili_core::ffmpeg::find_ffmpeg(explicit_ffmpeg(&settings).as_deref()) {
            Some(ffmpeg_bin) => {
                let tmp = out_file.with_extension("src");
                if tokio::fs::rename(&out_file, &tmp).await.is_ok() {
                    match bili_core::ffmpeg::to_mp3(&ffmpeg_bin, &tmp, &out_file).await {
                        Ok(_) => {
                            tokio::fs::remove_file(&tmp).await.ok();
                        }
                        Err(e) => {
                            settings.log("warn", &format!("转 MP3 失败，保留原格式（{source_ext}）: {e}"));
                            tokio::fs::rename(&tmp, out_file.with_extension(source_ext)).await.ok();
                        }
                    }
                }
            }
            None => settings.log("warn", "选了 MP3 但没找到 ffmpeg，保留原格式"),
        }
    }

    let path = final_path.to_string_lossy().to_string();
    settings.log("info", &format!("音频下载完成: {} -> {path}", req.title));
    mutate(&shared, &app, |t| {
        t.status = TaskStatus::Done;
        t.video_pct = 100.0;
        t.audio_pct = 100.0;
        t.output_path = path.clone();
        t.message = "音频已保存".to_string();
    });
    Ok(())
}

/// 正文文件：标题、话题、发布信息、正文、图片清单。
fn compose_opus_text(post: &bili_core::opus::OpusPost, opus_id: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    if !post.title.is_empty() {
        lines.push(post.title.clone());
    }
    if !post.topic.is_empty() {
        lines.push(format!("话题：{}", post.topic));
    }
    if !post.pub_time.is_empty() {
        lines.push(format!("发布：{}", post.pub_time));
    }
    lines.push(format!("链接：https://www.bilibili.com/opus/{opus_id}"));
    lines.push(String::new());
    lines.push(post.text.clone());
    if !post.images.is_empty() {
        lines.push(String::new());
        lines.push(format!("图片 {} 张（文件名为序号）", post.images.len()));
    }
    lines.join("\n") + "\n"
}

/// 把内存里的一张图转成 JPEG 字节（临时文件进出，转完即删）。
async fn jpeg_bytes(
    ffmpeg: &Path,
    bytes: &[u8],
    source_ext: &str,
    dir: &Path,
    index: usize,
) -> Result<Vec<u8>, BiliError> {
    let tmp_in = dir.join(format!(".img{index}.src.{source_ext}"));
    let tmp_out = dir.join(format!(".img{index}.out.jpg"));
    tokio::fs::write(&tmp_in, bytes).await?;
    let result = bili_core::ffmpeg::to_jpg(ffmpeg, &tmp_in, &tmp_out).await;
    let out = match result {
        Ok(_) => Ok(tokio::fs::read(&tmp_out).await?),
        Err(e) => Err(e),
    };
    tokio::fs::remove_file(&tmp_in).await.ok();
    tokio::fs::remove_file(&tmp_out).await.ok();
    out
}

/// 按「图片格式」把一张图写成与视频同名的文件：默认原样落盘；
/// 选了 JPG 且原图不是 jpg 时转一道，转不了就写原图（不让任务失败）。
async fn write_image_file(
    settings: &crate::state::Settings,
    bytes: &[u8],
    source_ext: &str,
    out_file: &Path,
) -> Result<PathBuf, BiliError> {
    if settings.image_format == "jpg" && source_ext != "jpg" {
        if let Some(ffmpeg) = bili_core::ffmpeg::find_ffmpeg(explicit_ffmpeg(settings).as_deref()) {
            // 临时名要留正经扩展名：ffmpeg 靠扩展名推断封装，`.tmp` 会直接报错
            let tmp_in = out_file.with_extension(format!("src.{source_ext}"));
            let tmp_out = out_file.with_extension("out.jpg");
            tokio::fs::write(&tmp_in, bytes).await?;
            let converted = bili_core::ffmpeg::to_jpg(&ffmpeg, &tmp_in, &tmp_out).await;
            tokio::fs::remove_file(&tmp_in).await.ok();
            match converted {
                Ok(_) => {
                    let path = out_file.with_extension("jpg");
                    if let Err(e) = tokio::fs::rename(&tmp_out, &path).await {
                        settings.log("warn", &format!("转 JPG 失败，保留原格式: {e}"));
                    } else {
                        return Ok(path);
                    }
                }
                Err(e) => settings.log("warn", &format!("转 JPG 失败，保留原格式: {e}")),
            }
            tokio::fs::remove_file(&tmp_out).await.ok();
        }
    }
    let path = out_file.with_extension(source_ext);
    tokio::fs::write(&path, bytes).await?;
    Ok(path)
}

/// 音频扩展名：URL 结尾是 .m4a/.mp3 就用它，否则按常见的 m4a。
fn audio_ext(url: &str) -> &'static str {
    let path = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    for ext in ["m4a", "mp3", "aac", "flac", "wav"] {
        if path.ends_with(&format!(".{ext}")) {
            return match ext {
                "m4a" => "m4a",
                "mp3" => "mp3",
                "aac" => "aac",
                "flac" => "flac",
                _ => "wav",
            };
        }
    }
    "m4a"
}

/// 图片扩展名：优先信响应头，其次看 URL 后缀。
fn image_ext(url: &str, content_type: &str) -> &'static str {
    let ctype = content_type.to_ascii_lowercase();
    if ctype.contains("png") {
        return "png";
    }
    if ctype.contains("webp") {
        return "webp";
    }
    if ctype.contains("gif") {
        return "gif";
    }
    if ctype.contains("jpeg") || ctype.contains("jpg") {
        return "jpg";
    }
    let path = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    for ext in ["png", "webp", "gif", "jpg", "jpeg"] {
        if path.ends_with(&format!(".{ext}")) {
            return if ext == "jpeg" { "jpg" } else { ext };
        }
    }
    "jpg"
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

    // 图文/专栏没有音视频流：内容是图片与正文，走另一条路径
    if req.source == "opus" || req.source == "article" {
        return run_opus_download(app, client, settings, output_dir, req, shared).await;
    }
    // 音频下载的是音频流本身（m4a），不需要合并
    if req.source == "audio" {
        return run_audio_download(app, client, settings, output_dir, req, shared).await;
    }

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
    // 设置里**没自定义**优先顺序（列表为空）时另一条路：直接按媒体页的「视频清晰度」
    // 单值挑（不高于该档取最高，0 = 最优画质），编码偏好用 codec_pref。
    let mut chain: Vec<(u32, String)> = Vec::new();
    if !settings.quality_prefs.is_empty() {
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
    }

    // 任务选的档位优先，没选就用设置里的「视频清晰度」
    let target_qn = if req.quality > 0 {
        req.quality
    } else {
        settings.default_quality
    };
    let video = if chain.is_empty() {
        play.pick_video(target_qn, &settings.codec_pref)
            .ok_or(BiliError::QualityNotFound(target_qn))?
    } else {
        play.pick_video_chain(&chain, &settings.quality_fallback)
            .ok_or(BiliError::QualityNotFound(req.quality))?
    };

    // 「目标质量不可用」策略：fail 时请求档位没拿到就直接失败
    if settings.quality_fallback == "fail" && target_qn > 0 && video.id < target_qn {
        settings.log(
            "warn",
            &format!(
                "任务失败：{} 未提供 qn={}（{}）",
                req.title, target_qn, req.bvid
            ),
        );
        return Err(BiliError::QualityNotFound(target_qn));
    }

    // 音轨：自定义优先顺序 > 任务里选过的 > 媒体页的「音频质量」
    let mut audio_chain = settings.audio_prefs.clone();
    if audio_chain.is_empty() {
        audio_chain = if req.audio.trim().is_empty() || req.audio == "auto" {
            vec![settings.default_audio.clone()]
        } else {
            vec![req.audio.clone()]
        };
        if audio_chain[0].trim().is_empty() {
            audio_chain = vec!["auto".to_string()];
        }
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
    let mut naming = naming_context(req, &video.codecs, quality_name(video.id));
    // UP 投稿/收藏夹里的视频，按它**实际所属的合集**分层，而不是按来源名。
    //
    // 为什么逐条查、不先枚举 UP 的全部合集：合集列表接口连查十几个就被风控
    // （实测 code=-352，且会持续一段时间），而视频详情本来就随下载逐条请求，
    // 节奏天然安全。查不到（不属于任何合集）就保持前端给的默认层。
    if matches!(req.source.as_str(), "space" | "fav") && !req.bvid.is_empty() {
        if let Some(name) = client.collection_of(&req.bvid).await {
            naming.collection_title = name;
        }
    }    tokio::fs::create_dir_all(&output_dir).await?;
    // 目录 = 文件夹层级（UP/合集…）+ 文件名模板；层级模板为空时前缀为空路径
    let out_file = output_dir
        .join(settings.output_folder_template(&naming))
        .join(settings.output_filename(&naming));
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
                // 视频不重下，但封面/字幕/弹幕这些旁挂文件该补还得补
                // （用户可能是后来才勾上的）
                write_sidecars(&client, &settings, &req, &out_file).await;
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
    // 未完成时留下原始请求：下次启动（或重新下这条）能接着下，见 resume_pending
    if let Ok(text) = serde_json::to_string(req) {
        tokio::fs::write(work_dir.join("task.json"), text).await.ok();
    }
    let video_opts = DownloadOptions {
        concurrency: settings.chunk_concurrency,
        chunk_size: settings.chunk_mb * 1024 * 1024,
        retries: settings.retry_count as usize,
        speed_limit_bps: settings.speed_limit_mib as u64 * 1024 * 1024,
        resume_log: Some(work_dir.join("video.ranges")),
    };
    let audio_opts = DownloadOptions {
        resume_log: Some(work_dir.join("audio.ranges")),
        ..video_opts.clone()
    };
    let throttle = Throttle::new(video_opts.speed_limit_bps);

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
            &video_opts,
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
            &audio_opts,
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

    let container = Container::parse(&settings.container);
    ffmpeg::merge_video_audio(
        &ffmpeg_bin,
        &video_path,
        &audio_path,
        &out_file,
        container,
        is_hevc,
    )
    .await?;
    if !settings.keep_temp {
        tokio::fs::remove_dir_all(&work_dir).await.ok();
    }

    // 封面 / 字幕 / 弹幕：与视频同名的独立文件，都不合成进视频
    write_sidecars(&client, &settings, &req, &out_file).await;

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

/// 与视频同名的旁挂文件：封面图片、字幕 SRT、弹幕 XML。
///
/// 三样都**不合成进视频**，失败只记 warn —— 旁挂文件拿不到不该让整条任务失败。
async fn write_sidecars(
    client: &bili_core::client::BiliClient,
    settings: &crate::state::Settings,
    req: &crate::types::DownloadRequest,
    out_file: &Path,
) {
    // 弹幕与字幕都要 cid：任务里带了就用，没带（空间投稿）按 bvid 补查
    let cid = if req.cid > 0 {
        req.cid
    } else if req.bvid.is_empty() {
        0
    } else {
        client
            .video_info(&req.bvid)
            .await
            .map(|info| info.cid)
            .unwrap_or(0)
    };

    if settings.download_danmaku {
        if cid == 0 {
            settings.log("info", "这条没有弹幕（音频/图文或查不到 cid），跳过");
        } else {
            match client.danmaku_full(cid).await {
                Ok((xml, count, truncated)) => {
                    let dm_path = out_file.with_extension("xml");
                    match tokio::fs::write(&dm_path, xml.as_bytes()).await {
                        Ok(_) => settings.log(
                            "info",
                            &format!(
                                "弹幕已保存（{count} 条{}）: {}",
                                if truncated {
                                    "，接口只给了这一部分"
                                } else {
                                    ""
                                },
                                dm_path.display()
                            ),
                        ),
                        Err(e) => settings.log("warn", &format!("弹幕写入失败: {e}")),
                    }
                }
                Err(e) => settings.log("warn", &format!("弹幕获取失败（不影响视频）: {e}")),
            }
        }
    }

    if settings.download_cover {
        // 前端解析时带过来的是 data URL；番剧这类没带的，自己按 bvid 取一次封面地址
        let mut cover_data = req.cover.clone();
        if cover_data.is_empty() && !req.bvid.is_empty() {
            match client.video_info(&req.bvid).await {
                Ok(info) if !info.pic.trim().is_empty() => {
                    match client.fetch_bytes(&info.pic).await {
                        Ok((bytes, content_type)) => {
                            let ext = image_ext(&info.pic, &content_type);
                            match write_image_file(settings, &bytes, ext, out_file).await {
                                Ok(path) => settings
                                    .log("info", &format!("封面已保存: {}", path.display())),
                                Err(e) => settings.log("warn", &format!("封面写入失败: {e}")),
                            }
                            cover_data.clear();
                        }
                        Err(e) => {
                            settings.log("warn", &format!("封面下载失败（不影响视频）: {e}"))
                        }
                    }
                }
                Ok(_) => settings.log("info", "这条没有封面可取，跳过"),
                Err(e) => settings.log("warn", &format!("封面地址获取失败: {e}")),
            }
        }
        if !cover_data.is_empty() {
            match write_cover_file(settings, &cover_data, out_file).await {
                Ok(path) => settings.log("info", &format!("封面已保存: {}", path.display())),
                Err(e) => settings.log("warn", &format!("封面保存失败（不影响视频）: {e}")),
            }
        }
    }

    if settings.download_subtitles {
        if cid == 0 {
            settings.log("info", "这条没有字幕可取（音频/图文或查不到 cid），跳过");
        } else {
            let bvid = (!req.bvid.is_empty()).then(|| req.bvid.clone());
            match client.subtitles(bvid.as_deref(), req.ep_id, cid).await {
                Ok(list) if list.is_empty() => settings
                    .log("info", "这条没有可用字幕（没有人工字幕，AI 字幕也还没生成），跳过"),
                Ok(list) => {
                    let mut saved = 0usize;
                    for (index, item) in list.iter().enumerate() {
                        let json = match client.subtitle_text(&item.subtitle_url).await {
                            Ok(json) => json,
                            Err(e) => {
                                settings
                                    .log("warn", &format!("字幕下载失败（{}）: {e}", item.lan_doc));
                                continue;
                            }
                        };
                        let srt = match bili_core::subtitle::srt_from_json(&json) {
                            Ok(srt) => srt,
                            Err(e) => {
                                settings
                                    .log("warn", &format!("字幕转换失败（{}）: {e}", item.lan_doc));
                                continue;
                            }
                        };
                        if srt.trim().is_empty() {
                            continue;
                        }
                        // 第一条用同名 .srt（播放器认这个），其余带语言后缀
                        let path = if index == 0 {
                            out_file.with_extension("srt")
                        } else {
                            let tag = bili_core::subtitle::lang_tag(item);
                            out_file.with_extension(format!("{tag}.srt"))
                        };
                        match tokio::fs::write(&path, srt.as_bytes()).await {
                            Ok(_) => {
                                saved += 1;
                                settings.log(
                                    "info",
                                    &format!("字幕已保存（{}）: {}", item.lan_doc, path.display()),
                                );
                            }
                            Err(e) => settings.log("warn", &format!("字幕写入失败: {e}")),
                        }
                    }
                    if saved == 0 {
                        settings.log("info", "这条视频的字幕都是空的，没写出文件");
                    }
                }
                Err(e) => settings.log("warn", &format!("字幕清单获取失败（不影响视频）: {e}")),
            }
        }
    }
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
/// 封面存成与视频同名的独立图片文件（`<视频名>.jpg|png|webp`）。
/// 内容就是解析阶段前端带过来的 data URL，不再需要另外请求。
async fn write_cover_file(
    settings: &crate::state::Settings,
    data_url: &str,
    video_path: &Path,
) -> Result<PathBuf, BiliError> {
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
    write_image_file(settings, &bytes, ext, video_path).await
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

/// 一次重命名动作的结果（预演与实操共用）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RenamePlan {
    pub renamed: usize,
    pub skipped: usize,
    pub missing: usize,
    /// 逐条明细：旧名 → 新名；跳过/找不到也记一行，便于用户核对
    pub details: Vec<String>,
    /// 是否是预演（不落盘）
    pub dry_run: bool,
}

/// 按当前命名规则，把已下载的条目重命名成新编号。
///
/// 只改"条目名"（编号 + 标题），**不动目录层级**：层级由「文件夹」规则决定，
/// 而磁盘上的层级是历史结果，重算它需要重新联网确认每个条目的合集归属，
/// 代价与风险都不划算（密集请求会触发风控）。想调整层级请重新下载。
///
/// 匹配方式：在输出目录里递归找"去掉数字前缀后与条目标题相同"的文件或文件夹，
/// 命中就原地改名。找不到的条目记进 missing，不做任何猜测。
#[tauri::command]
pub async fn rename_downloaded(
    state: State<'_, AppState>,
    input: String,
    dry_run: bool,
) -> Result<RenamePlan, String> {
    let key = input.trim().to_string();
    let cache = state
        .peek_batch(&key)
        .ok_or_else(|| "这个来源的解析结果已过期，请重新解析后再试".to_string())?;
    let output_dir = state.output_dir();
    let settings = state.settings();

    // 编号与前端一致：总数 - 绝对位置 + 1（番剧/课程按集数顺序，不倒）
    let episode = matches!(cache.target, BatchTarget::Whole);
    let count = cache.items.len();
    let total = if cache.total > 0 { cache.total } else { count };
    let pad = format!("{total}").len().max(2);

    let mut plan = RenamePlan {
        dry_run,
        ..Default::default()
    };

    for (position, item) in cache.items.iter().enumerate() {
        let absolute = cache.from_index + position;
        let index = if episode {
            absolute
        } else {
            total.saturating_sub(absolute) + 1
        };
        let ctx = crate::naming::NamingContext {
            title: item.title.clone(),
            owner_name: item.owner.clone(),
            collection_title: String::new(),
            source_kind: kind_label(&cache.kind).to_string(),
            index: index as u32,
            index_pad: pad as u32,
            ..Default::default()
        };
        // 条目名：视频/音频是文件名，图文/专栏是文件夹名
        let wanted = if cache.kind == "opus" || cache.kind == "article" {
            crate::naming::render_dir(&settings.naming_template, &ctx)
        } else {
            settings.output_filename(&ctx)
        };
        let mut wanted = wanted
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if wanted.is_empty() {
            continue;
        }

        let found = find_downloaded(&output_dir, &item.title, &wanted).await;
        // 扩展名沿用原文件：音频是 m4a、视频是 mp4/mkv，按封装去猜会把音频改成 .mp4
        if let Some(found) = &found {
            if let (Some(old_ext), Some(_)) = (found.extension(), Path::new(&wanted).extension()) {
                let old_ext = old_ext.to_string_lossy().to_string();
                if !old_ext.is_empty() {
                    wanted = format!("{}.{old_ext}", Path::new(&wanted).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                }
            }
        }

        match found {
            Some(found) => {
                if found.file_name().map(|s| s.to_string_lossy() == wanted).unwrap_or(false) {
                    plan.skipped += 1;
                    continue;
                }
                let target = found.with_file_name(&wanted);
                if target.exists() {
                    plan.skipped += 1;
                    plan.details.push(format!("跳过（同名已存在）：{wanted}"));
                    continue;
                }
                let from = found.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                plan.details.push(format!("{from}  →  {wanted}"));
                if !dry_run {
                    tokio::fs::rename(&found, &target)
                        .await
                        .map_err(|e| format!("重命名失败 {from}: {e}"))?;
                }
                plan.renamed += 1;
            }
            None => {
                plan.missing += 1;
            }
        }
    }
    Ok(plan)
}

/// 在输出目录里递归找"去掉数字前缀后与标题相同"的文件或文件夹。
///
/// 只认两种形态：`标题` 与 `数字前缀 + 标题（可带扩展名）`，其余一律不动 ——
/// 宁可少改，也不猜错。
async fn find_downloaded(root: &Path, title: &str, wanted: &str) -> Option<PathBuf> {
    let wanted_stem = Path::new(wanted)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| wanted.to_string());
    // 新名字里可能带编号前缀，比对时要把它去掉
    let wanted_title = strip_number_prefix(&wanted_stem);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                stack.push(path.clone());
            }
            let stem = match path.file_stem() {
                Some(stem) if !is_dir => stem.to_string_lossy().to_string(),
                _ => name.clone(),
            };
            let bare = strip_number_prefix(&stem);
            if bare == wanted_title || bare == title || stem == wanted_stem {
                return Some(path);
            }
        }
    }
    None
}

/// 去掉开头的"编号 + 分隔符"：`001 标题` / `12-标题` / `3 标题` 都还原成标题。
fn strip_number_prefix(name: &str) -> String {
    let trimmed = name.trim_start();
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return trimmed.trim().to_string();
    }
    trimmed[digits.len()..]
        .trim_start_matches([' ', '-', '_', '.', '、', '·'])
        .trim()
        .to_string()
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
            opus_id: String::new(),
            au_id: String::new(),
            owner: "UP主".to_string(),
            quality: 116,
            audio: "normal".to_string(),
            cover: String::new(),
            naming: crate::types::NamingMeta {
                source_kind: String::new(),
                index_pad: 0,
                tz_offset_min: 0,
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
    fn batch_exhausted_handles_whole_and_paged_sources() {
        // 番剧/课程一次给全，不管多少条都是到底
        assert!(batch_exhausted(BatchTarget::Whole, 22, 22));
        assert!(batch_exhausted(BatchTarget::Whole, 5, 5));

        // 收藏夹每页 20：首页 20 条而总数 130，还能继续
        assert!(!batch_exhausted(BatchTarget::Fav(1), 20, 130));
        // 最后一页：已经到总数，或这一页没取满
        assert!(batch_exhausted(BatchTarget::Fav(1), 130, 130));
        assert!(batch_exhausted(BatchTarget::Fav(1), 15, 15));

        // 合集每页 100：100 条而总数 129，还能继续；129 条就到头
        assert!(!batch_exhausted(
            BatchTarget::Collection { mid: 1, sid: 2 },
            100,
            129
        ));
        assert!(batch_exhausted(
            BatchTarget::Collection { mid: 1, sid: 2 },
            129,
            129
        ));
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

    #[test]
    fn range_slice_maps_index_to_page_and_offset() {
        // UP 空间每页 30 条
        assert_eq!(range_slice(1, 30), (1, 0));
        assert_eq!(range_slice(30, 30), (1, 29));
        assert_eq!(range_slice(31, 30), (2, 0));
        // 第二次分批：从第 301 条起正好是第 11 页开头
        assert_eq!(range_slice(301, 30), (11, 0));
        assert_eq!(range_slice(305, 30), (11, 4));
        // 合集每页 100 条
        assert_eq!(range_slice(501, 100), (6, 0));
        // 越界与 0 都按第 1 条处理，不 panic
        assert_eq!(range_slice(0, 30), (1, 0));
    }

    #[test]
    fn source_cap_prefers_user_setting() {
        // 没填（0）时按来源类型给默认值
        assert_eq!(source_cap("space", 0), SPACE_MAX_ITEMS);
        assert_eq!(source_cap("fav", 0), FAV_MAX_ITEMS);
        // 填了就一律用它，不再区分来源类型
        assert_eq!(source_cap("space", 1337), 1337);
        assert_eq!(source_cap("fav", 1337), 1337);
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
            false,
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

    /// 实测系列解析：`/lists/{id}?type=series` 必须走系列接口。
    ///
    /// 这个 id 当作合集查会返回**别人的**合集（2 条、标题是"中级经济师…"）
    /// 而且接口不报错——所以这里同时断言标题与条数，防止再退回错配。
    #[tokio::test]
    #[ignore = "需要网络与登录态"]
    async fn live_probe_series_not_mistaken_for_collection() {
        let client = client();
        let probe = probe_one(
            &client,
            &app_state(),
            "https://space.bilibili.com/486287787/lists/90946?type=series",
            true,
        )
        .await
        .expect("解析成功");
        assert_eq!(probe.kind, "series");
        assert_eq!(probe.title, "暗中观察", "系列标题应来自系列接口");
        assert!(probe.total > 400, "系列共 {} 条，不像 90946", probe.total);
        assert!(probe.loaded > 0);
        assert_ne!(probe.loaded, 2, "2 条说明又退回成按合集查了");
        println!(
            "series: {} loaded={} total={} 首条={:?}",
            probe.title, probe.loaded, probe.total, probe.items[0].title
        );
    }

    /// 实测 UP 空间来源的标题与 UP 名（子文件夹用 {collection_title}，不能是空的"的投稿"）。
    #[tokio::test]
    #[ignore = "需要网络与登录态"]
    async fn live_space_source_carries_owner_name() {
        let client = client();
        for url in [
            "https://space.bilibili.com/486287787/video",
            "https://space.bilibili.com/927587/video",
        ] {
            let probe = probe_one(&client, &app_state(), url, false).await.expect("解析成功");
            println!("{} -> title={:?} owner={:?} total={}", url, probe.title, probe.owner, probe.total);
            assert!(!probe.owner.trim().is_empty(), "{url} 的 UP 名是空的");
            assert!(
                !probe.title.trim().starts_with("的投稿"),
                "{url} 的标题丢了 UP 名：{:?}",
                probe.title
            );
        }
    }

    /// 实测音频列表与播放地址（拿一个确实有音频的 UP）。
    ///
    /// 这个接口少了 order/platform 会"看起来没有音频"，所以断言条数也要断言地址。
    #[tokio::test]
    #[ignore = "需要网络与登录态"]
    async fn live_probe_audio_list_and_stream() {
        let client = client();
        let probe = probe_one(
            &client,
            &app_state(),
            "https://space.bilibili.com/649910/upload/audio",
            false,
        )
        .await
        .expect("解析成功");
        assert_eq!(probe.kind, "audio");
        assert!(probe.loaded > 0, "应加载到音频");
        assert!(probe.total > 200, "配音木成的音频应有两百多条，实际 {}", probe.total);
        assert!(probe.items.iter().all(|item| !item.au_id.is_empty()));

        let first = probe.items[0].clone();
        let sid: u64 = first.au_id.parse().expect("音频 id");
        let stream = client.audio_stream(sid).await.expect("播放地址");
        assert!(!stream.cdns.is_empty(), "拿不到可下载地址");
        assert!(stream.cdns[0].starts_with("http"));
        println!(
            "audio: {} 条；首条 {:?}（{} 秒）地址 {:?}",
            probe.total,
            first.title,
            first.duration,
            &stream.cdns[0][..stream.cdns[0].len().min(80)]
        );
    }

    /// 实测"全部解析"能翻到底：音频接口报的总数大于实取条数，
    /// 末页之后返回 data: null——原来这里会报格式异常。
    #[tokio::test]
    #[ignore = "需要网络与登录态"]
    async fn live_audio_paging_reaches_the_end_without_decode_error() {
        let client = client();
        let state = app_state();
        let url = "https://space.bilibili.com/649910/upload/audio";
        let probe = probe_one(&client, &state, url, false).await.expect("解析成功");
        assert_eq!(probe.kind, "audio");

        let mut cache = state.take_batch(url).expect("缓存");
        let declared = cache.total;
        extend_batch(&client, &mut cache, 400).await.expect("翻到底不该报错");
        let loaded = cache.items.len();
        let exhausted = cache.exhausted;
        state.put_batch(url.to_string(), cache.clone());
        let note = batch_to_source(&state.peek_batch(url).expect("缓存")).note;
        assert!(exhausted, "应标记为已到底");
        assert!(loaded < declared, "这个接口报数 {declared} 大于实取 {loaded}，正是触发条件");
        assert!(note.contains("可下载"), "应说明实际可下载条数：{note}");
        println!("audio 翻到底: 报数 {declared}，实取 {loaded}；提示 {note}");
    }

    /// 实测图文列表与单条图文内容（图片、正文）。
    #[tokio::test]
    #[ignore = "需要网络与登录态"]
    async fn live_probe_opus_list_and_post() {
        let client = client();
        let state = app_state();
        let probe = probe_one(
            &client,
            &state,
            "https://space.bilibili.com/486287787/upload/opus",
            false,
        )
        .await
        .expect("解析成功");
        assert_eq!(probe.kind, "opus");
        assert!(probe.loaded > 0, "应加载到图文");
        assert!(probe.items.iter().all(|item| !item.opus_id.is_empty()));
        assert_eq!(probe.total, 0, "图文接口不给总数");

        // 再往下一页翻（游标）
        let mut cache = state.take_batch("https://space.bilibili.com/486287787/upload/opus").expect("缓存");
        extend_batch(&client, &mut cache, 3).await.expect("翻页");
        assert!(cache.items.len() > 20, "游标翻页应拿到更多条目，实际 {}", cache.items.len());

        // 单条：正文与原图
        let first = probe.items[0].clone();
        let html = client.opus_page(&first.opus_id).await.expect("图文页面");
        let post = bili_core::opus::parse_page(&html).expect("解出图文");
        assert!(!post.text.is_empty() || !post.images.is_empty());
        println!(
            "opus: {} 条已加载；首条 {} | 图片 {} 张 | 正文 {} 字 | 标题 {:?}",
            cache.items.len(),
            first.title,
            post.images.len(),
            post.text.chars().count(),
            post.title
        );
    }

    /// 实测 UP 空间解析。
    #[tokio::test]
    #[ignore = "需要网络与登录态"]
    async fn live_probe_space() {
        let client = client();
        let probe = probe_one(
            &client,
            &app_state(),
            "https://space.bilibili.com/1858731",
            false,
        )
        .await
        .expect("解析成功");
        assert_eq!(probe.kind, "space");
        assert!(probe.loaded > 0);
        println!(
            "space: {} loaded={} 首条={:?}",
            probe.title, probe.loaded, probe.items[0].title
        );
    }

    /// 实测按序号加载：拉满单次上限后，从第 301 条再取一批，两批不重叠。
    ///
    /// 这是"超过上限的来源怎么下完"的核心保证：能分两次拉，就不会重复下载。
    #[tokio::test]
    #[ignore = "需要网络"]
    async fn live_probe_range_does_not_overlap_capped_batch() {
        let client = client();
        let state = app_state();
        let url = "https://space.bilibili.com/927587/video";

        probe_one(&client, &state, url, false).await.expect("首次解析");
        let mut cache = state.take_batch(url).expect("首屏缓存");
        extend_batch(&client, &mut cache, SPACE_MAX_ITEMS * 2)
            .await
            .expect("拉到上限");
        let capped = cache.items.len();
        state.put_batch(url.to_string(), cache);
        let head: Vec<String> = state
            .peek_batch(url)
            .expect("缓存")
            .items
            .iter()
            .map(|item| item.bvid.clone())
            .collect();
        assert!(state.peek_batch(url).expect("缓存").exhausted, "到上限后应标记已拉完");
        assert_eq!(head.len(), capped);

        let second = probe_range_one(&client, &state, url, capped + 1)
            .await
            .expect("按序号加载");
        assert_eq!(second.from_index, capped + 1);
        assert!(second.loaded > 0);
        let tail: Vec<String> = second.items.iter().map(|item| item.bvid.clone()).collect();
        assert!(
            head.iter().all(|bvid| !tail.contains(bvid)),
            "第二批不应与第一批重叠"
        );
        println!(
            "第一批 {capped} 条（1-{capped}），第二批 {} 条（{}-{}），总数 {}",
            second.loaded,
            second.from_index,
            second.from_index + second.loaded - 1,
            second.total
        );
    }

    /// 实测番剧解析（鲁邦三世 第六季）。
    #[tokio::test]
    #[ignore = "需要网络"]
    async fn live_probe_bangumi() {
        let client = client();
        let probe = probe_one(&client, &app_state(), "https://www.bilibili.com/bangumi/play/ss39468", false)
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
        let probe = probe_one(&client, &app_state(), "https://www.bilibili.com/cheese/play/ss1", false)
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

        let first = probe_one(&client, &state, input, false).await.expect("首页解析");
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
            false,
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
            let space = probe_one(&client, &app_state(), &format!("https://space.bilibili.com/{mid}"), false)
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
            let collection = probe_one(&client, &app_state(), &url, false).await.expect("合集解析成功");
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
