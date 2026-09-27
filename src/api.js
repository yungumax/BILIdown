// 与 Rust 后端通信的唯一入口。
//
// 在浏览器里直接打开时（没有 Tauri 运行时）自动切换到假数据，
// 这样界面可以脱离桌面壳单独预览与调整。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const hasTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const TASK_EVENT = "task://update";

export async function appStatus() {
  if (!hasTauri) return mock.status();
  return invoke("app_status");
}

export async function appSettings() {
  if (!hasTauri) return mock.settings();
  return invoke("app_settings");
}

export async function updateSettings(settings) {
  if (!hasTauri) return mock.updateSettings(settings);
  return invoke("update_settings", { settings });
}

// preferCollection：批量解析模式下，单个视频链接会展开成它所在的合集
export async function probeSource(input, preferCollection = false) {
  if (!hasTauri) return mock.probe(input);
  return invoke("probe_source", { input, preferCollection });
}

// 继续解析：往后多拉 want 条。首次解析只给第一页，避免一上来就拉上千条。
export async function probeMore(input, want) {
  if (!hasTauri) return mock.probeMore(input, want);
  return invoke("probe_more", { input, want });
}

// 按序号加载：从第 from 条开始重新取一批。超过单次上限的来源靠它分批拉完，
// 两批互不重叠（1–300、301–600……），因此不会重复下载。
export async function probeRange(input, from) {
  if (!hasTauri) return mock.probeRange(input, from);
  return invoke("probe_range", { input, from });
}

export async function startDownload(req) {
  if (!hasTauri) return mock.start(req);
  return invoke("start_download", { req });
}

export async function cancelDownload(taskId) {
  if (!hasTauri) return mock.cancel(taskId);
  return invoke("cancel_download", { taskId });
}

export async function loginQrcode() {
  if (!hasTauri) return mock.qrcode();
  return invoke("login_qrcode");
}

export async function loginPoll(qrcodeKey) {
  if (!hasTauri) return mock.poll();
  return invoke("login_poll", { qrcodeKey });
}

export async function logout() {
  if (!hasTauri) return emptyLogin();
  return invoke("logout");
}

export async function chooseOutputDir() {
  if (!hasTauri) return mock.status().output_dir;
  return invoke("choose_output_dir");
}

export async function setOutputDir(dir) {
  if (!hasTauri) return dir;
  return invoke("set_output_dir", { dir });
}

export async function pickFfmpeg() {
  if (!hasTauri) return "";
  return invoke("pick_ffmpeg");
}

// ffmpeg 探测要起子进程（约 0.8 秒），单独调用，不拖慢设置读取与主题生效。
// refresh=true 时忽略缓存重新探测（用户换了 ffmpeg 路径后用）。
export async function ffmpegStatus(refresh = false) {
  if (!hasTauri)
    return { ok: true, info: "ffmpeg version 6.1.1-essentials_build-www.gyan.dev" };
  return invoke("ffmpeg_status", { refresh });
}

// 「魔法变量」清单由后端提供，界面不再自己写一份——否则界面会列出后端不支持的变量。
export async function namingVariables() {
  if (!hasTauri) return mock.namingVariables();
  return invoke("naming_variables");
}

// 批量文件名预览：给每条内容算出文件名，与真实落盘共用同一个渲染器。
export async function previewNames(items, quality, date, ext) {
  if (!hasTauri) return items.map((item) => `${item.title}.${ext || "mp4"}`);
  return invoke("preview_names", { items, quality, date, ext });
}

// 文件名预览走后端同一个渲染器，预览与真实落盘不会不一致。
export async function previewNaming(template, { date, publish_date, ext } = {}) {
  if (!hasTauri) return mock.previewNaming(template, ext);
  return invoke("preview_naming", { template, date, publish_date, ext });
}

export async function cleanupTemp() {
  if (!hasTauri) return 0;
  return invoke("cleanup_temp");
}

export async function cleanupCache() {
  if (!hasTauri) return;
  return invoke("cleanup_cache");
}

export async function exportDiagnostics() {
  if (!hasTauri) return "";
  return invoke("export_diagnostics");
}

export async function checkUpdates() {
  if (!hasTauri)
    return { current: "0.1.0", latest: "", up_to_date: true, error: "" };
  return invoke("check_updates");
}

export async function openPath(path) {
  if (!hasTauri) return;
  return invoke("open_path", { path });
}

export async function onTaskUpdate(handler) {
  if (!hasTauri) return mock.onUpdate(handler);
  return listen(TASK_EVENT, (event) => handler(event.payload));
}

export async function readClipboard() {
  if (!hasTauri) return "";
  try {
    return await navigator.clipboard.readText();
  } catch {
    return "";
  }
}

// ---- 窗口控制（自定义标题栏用）----

async function windowApi() {
  if (!hasTauri) return null;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export async function minimizeWindow() {
  const win = await windowApi();
  await win?.minimize();
}

export async function toggleMaximizeWindow() {
  const win = await windowApi();
  await win?.toggleMaximize();
}

export async function closeWindow() {
  const win = await windowApi();
  await win?.close();
}


export async function showWindow() {
  if (!hasTauri) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().show();
  } catch {
    // 忽略：仅影响窗口显示时机
  }
}

export async function setWindowBackground(color) {
  if (!hasTauri) return;
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setBackgroundColor(color);
  } catch {
    // 旧运行时不支持时忽略；页面不透明，底色仅影响边缘
  }
}

export async function startWindowDrag() {
  const win = await windowApi();
  await win?.startDragging();
}

function emptyLogin() {
  return { logged_in: false, uname: "", mid: 0, vip: false, vip_label: "" };
}

// 仅浏览器预览用的假数据
const mock = (() => {
  const listeners = new Set();
  const timers = new Map();

  const status = () => ({
    version: "0.1.0",
    login: { logged_in: true, uname: "术缕", mid: 1858731, vip: true, vip_label: "年度大会员" },
    output_dir: "D:\\Zcode\\_data\\bilidown\\downloads",
    cookies_path: "D:\\Zcode\\_data\\bilidown\\cookies.json",
  });

  const settings = () => ({
    settings: {
      output_dir: "D:\Zcode\_data\bilidown\downloads",
      max_concurrent_tasks: 2,
      chunk_concurrency: 4,
      chunk_mb: 4,
      keep_temp: false,
      naming: "title",
      naming_template: "{title}",
      naming_presets: [
        { name: "示例收藏命名", template: "{title}_{bvid}" },
      ],
      rename_conflict: "skip",
      container: "mp4",
      codec_pref: "auto",
      quality_fallback: "nearest",
      embed_cover: false,
      embed_subtitles: false,
      retry_count: 3,
      speed_limit_mib: 0,
      auto_refresh_urls: true,
      resume_on_start: false,
      parse_preset: "标准",
      parse_batch: 8,
      parse_batch_wait_ms: 1000,
      parse_rest_every: 100,
      parse_rest_ms: 3000,
      ffmpeg_path: "",
      update_check: false,
      log_level: "info",
      data_dir: "",
      default_quality: 0,
      default_audio: "normal",
      quality_prefs: [{ qn: 127, codec: "auto" }],
      audio_prefs: ["auto"],
      proxy: "",
      theme: "system",
    },
    cookies_path: "D:\Zcode\_data\bilidown\cookies.json",
    cookies_saved: true,
    ffmpeg_ok: true,
    ffmpeg_info: "ffmpeg version 6.1.1-essentials_build-www.gyan.dev",
    version: "0.1.0",
  });

  const updateSettings = async (next) => {
    const base = settings();
    return { ...base, settings: { ...base.settings, ...next } };
  };

  function makeProbe(bvid, title, owner) {
    return {
      kind: "video",
      bvid,
      cid: 42178774115,
      title,
      owner,
      duration: 18,
      cover: "",
      page_count: 1,
      note: "",
      recommended_quality: 80,
      best_quality: 116,
      qualities: [
        { qn: 116, label: "高清 1080P60", available: true, hint: "" },
        { qn: 112, label: "高清 1080P+", available: true, hint: "" },
        { qn: 80, label: "高清 1080P", available: true, hint: "" },
        { qn: 64, label: "高清 720P", available: true, hint: "" },
        { qn: 32, label: "清晰 480P", available: true, hint: "" },
        { qn: 120, label: "超清 4K", available: false, hint: "需大会员" },
      ],
      audios: [
        { kind: "normal", label: "普通音轨 224 kbps", available: true },
        { kind: "dolby", label: "杜比全景声", available: false },
        { kind: "flac", label: "Hi-Res 无损", available: true },
      ],
    };
  }

  const probe = async (input) => {
    const lower = (input || "").toLowerCase();
    const qualities = [
      { qn: 116, label: "高清 1080P60", available: true, hint: "" },
      { qn: 112, label: "高清 1080P+", available: true, hint: "" },
      { qn: 80, label: "高清 1080P", available: true, hint: "" },
      { qn: 64, label: "高清 720P", available: true, hint: "" },
      { qn: 32, label: "清晰 480P", available: true, hint: "" },
      { qn: 120, label: "超清 4K", available: false, hint: "需大会员" },
    ];
    const audios = [
      { kind: "normal", label: "普通音轨 224 kbps", available: true },
      { kind: "dolby", label: "杜比全景声", available: false },
      { kind: "flac", label: "Hi-Res 无损", available: true },
    ];

    if (lower.includes("favlist")) {
      return {
        kind: "fav", title: "默认收藏夹", owner: "术缕", cover: "", note: "收藏夹共 6 条，已加载前 6 条",
        bvid: "", cid: 0, duration: 0, page_count: 1, total: 6, loaded: 6,
        qualities, audios, recommended_quality: 80, best_quality: 116,
        items: Array.from({ length: 6 }, (_, i) => ({
          bvid: `BV1Vkag6TEx${i}`, cid: 42178774115 + i, ep_id: null,
          title: `示例视频 ${i + 1}`, duration: 18 + i * 7,
        })),
      };
    }
    if (lower.includes("space") || lower.includes("lists")) {
      return {
        kind: "collection", title: "示例合集", owner: "示例UP主", cover: "", note: "",
        bvid: "", cid: 0, duration: 0, page_count: 1, total: 12, loaded: 12,
        qualities, audios, recommended_quality: 80, best_quality: 116,
        items: Array.from({ length: 12 }, (_, i) => ({
          bvid: `BV1Colle12ab${i}`, cid: 42178774115 + i, ep_id: null,
          title: `合集第 ${i + 1} 话`, duration: 120 + i * 30,
        })),
      };
    }
    if (lower.includes("bangumi")) {
      return {
        kind: "bangumi", title: "示例番剧", owner: "", cover: "", note: "",
        bvid: "", cid: 0, duration: 0, page_count: 1, total: 12, loaded: 12,
        qualities, audios, recommended_quality: 80, best_quality: 116,
        items: Array.from({ length: 12 }, (_, i) => ({
          bvid: `BV1Ep2156ab${i}`, cid: 42178774115 + i, ep_id: 219026 + i,
          title: `第 ${i + 1}话 示例剧集`, duration: 1420,
        })),
      };
    }
    if (lower.includes("cheese")) {
      return {
        kind: "cheese", title: "示例课程", owner: "", cover: "",
        note: "付费课程需要已购买并登录才能下载",
        bvid: "", cid: 0, duration: 0, page_count: 1, total: 8, loaded: 8,
        qualities, audios, recommended_quality: 80, best_quality: 80,
        items: Array.from({ length: 8 }, (_, i) => ({
          bvid: "", cid: 42178774115 + i, ep_id: 90001 + i,
          title: `第 ${i + 1} 节：示例课时`, duration: 900,
        })),
      };
    }
    return makeProbe("BV1Vkag6TExf", "今日份缇宝", "以尘动画");
  };

  function emit(task) {
    listeners.forEach((fn) => fn({ ...task }));
  }

  let counter = 0;

  const start = async (req) => {
    const id = `mock-${++counter}`;
    const task = {
      id,
      title: req.title,
      quality_label: "1080P60 AVC",
      status: "queued",
      video_pct: 0,
      audio_pct: 0,
      downloaded: 0,
      total: 17.2 * 1024 * 1024,
      speed_bps: 0,
      output_path: "",
      message: "排队中",
      cover: "",
    };
    const videoTotal = 16.7 * 1024 * 1024;
    const audioTotal = 0.5 * 1024 * 1024;
    emit(task);

    const state = { video: 0, audio: 0, phase: 0 };
    const timer = setInterval(() => {
      if (state.phase === 0) {
        state.video += videoTotal * 0.14;
        if (state.video >= videoTotal) {
          state.video = videoTotal;
          state.phase = 1;
        }
        task.video_pct = (state.video / videoTotal) * 100;
        task.status = "downloading";
        task.message = "下载视频流";
        task.speed_bps = 3.4 * 1024 * 1024;
      } else if (state.phase === 1) {
        state.audio += audioTotal * 0.3;
        if (state.audio >= audioTotal) {
          state.audio = audioTotal;
          state.phase = 2;
        }
        task.audio_pct = (state.audio / audioTotal) * 100;
        task.message = "下载音频流";
      } else {
        task.video_pct = 100;
        task.audio_pct = 100;
        task.status = "merging";
        task.message = "合成中";
        task.speed_bps = 0;
        task.output_path = `${status().output_dir}\\${req.title}.mp4`;
        clearInterval(timer);
        timers.delete(id);
        emit({ ...task });
        setTimeout(() => {
          task.status = "done";
          task.message = "已完成";
          emit({ ...task });
        }, 700);
        return;
      }
      task.downloaded = state.video + state.audio;
      emit({ ...task });
    }, 260);
    timers.set(id, timer);
    return id;
  };

  const cancel = async (taskId) => {
    const timer = timers.get(taskId);
    if (timer) clearInterval(timer);
    timers.delete(taskId);
  };

  const qrcode = async () => ({
    url: "https://account.bilibili.com/h5/account-h5/auth/scan-web?navhide=1&qrcode_key=preview",
    qrcode_key: "preview",
  });

  let polls = 0;
  const poll = async () => {
    polls += 1;
    if (polls < 3) return { state: "pending", login: emptyLogin() };
    if (polls === 3) return { state: "scanned", login: emptyLogin() };
    return { state: "confirmed", login: status().login };
  };

  // 浏览器预览用：从已解析的假清单里继续往后取
  const moreState = new Map();
  const probeMore = async (input, want) => {
    const base = await probe(input);
    const all = base.items || [];
    const from = moreState.get(input) ?? Math.min(20, all.length);
    const to = Math.min(from + want, all.length);
    moreState.set(input, to);
    return {
      items: all.slice(from, to),
      loaded: to,
      total: all.length,
      exhausted: to >= all.length,
      note: "",
    };
  };

  // 预览用：按序号加载从这一条切一段（真值由后端的 probe_range 决定）
  const probeRange = async (input, from) => {
    const base = await probe(input);
    const all = base.items || [];
    const start = Math.max(0, Math.min(from - 1, all.length));
    const items = all.slice(start, start + 300);
    moreState.set(input, start + items.length);
    return {
      ...base,
      items,
      loaded: items.length,
      total: all.length,
      from_index: start + 1,
      exhausted: start + items.length >= all.length,
      note: "",
    };
  };

  const onUpdate = async (handler) => {
    listeners.add(handler);
    return () => listeners.delete(handler);
  };
  // 仅浏览器预览用的兜底：真值在 Rust 的 naming::VARIABLES，
  // 桌面端一律走 naming_variables 命令，这份副本只影响脱离桌面壳的预览。
  const VARIABLES = [
    ["title", "视频或条目标题"],
    ["part_title", "分P标题"],
    ["part_index", "分P序号"],
    ["bvid", "BV号"],
    ["aid", "AV号"],
    ["cid", "CID"],
    ["owner_name", "UP主名称"],
    ["owner_mid", "UP主MID"],
    ["series_title", "番剧/课程/系列名"],
    ["episode_index", "集序号"],
    ["episode_title", "集标题"],
    ["collection_title", "合集名"],
    ["index", "列表序号"],
    ["quality", "清晰度"],
    ["codec", "编码"],
    ["date", "下载日期（任务创建日）"],
    ["publish_date", "发布时间（B站发布日期）"],
    ["ext", "扩展名"],
  ];

  const SAMPLE = {
    title: "示例视频",
    part_title: "分P标题",
    part_index: 1,
    bvid: "BV1xx411c7mD",
    aid: 12345,
    cid: 67890,
    owner_name: "示例UP主",
    owner_mid: 1234567,
    series_title: "示例系列",
    episode_index: 3,
    episode_title: "第 3 集",
    collection_title: "示例合集",
    index: 7,
    quality: "1080P60",
    codec: "AVC",
    date: "2026-09-26",
    publish_date: "2026-01-02",
  };

  const namingVariables = () => VARIABLES.map(([token, label]) => ({ token, label }));

  const previewNaming = (template, ext = "mp4") => {
    const segments = [];
    let usedExt = false;
    for (const raw of String(template ?? "").split(/[/\\]/)) {
      let out = "";
      let rest = raw;
      while (true) {
        const start = rest.indexOf("{");
        if (start === -1) {
          out += rest;
          break;
        }
        out += rest.slice(0, start);
        const end = rest.indexOf("}", start);
        if (end === -1) {
          out += rest.slice(start);
          break;
        }
        const token = rest.slice(start + 1, end);
        if (token === "ext") {
          usedExt = true;
          out += ext;
        } else if (token in SAMPLE) {
          out += SAMPLE[token] === "" ? "" : String(SAMPLE[token]);
        } else {
          out += `{${token}}`;
        }
        rest = rest.slice(end + 1);
      }
      const cleaned = out
        .replace(/[\\/:*?"<>|]/g, "_")
        .trim()
        .replace(/\.+$/, "")
        .trim();
      if (cleaned && cleaned !== "." && cleaned !== "..") segments.push(cleaned);
    }
    if (!usedExt) {
      if (segments.length) segments[segments.length - 1] += `.${ext}`;
      else segments.push(`video.${ext}`);
    }
    return segments.join("/");
  };

  return {
    status,
    settings,
    updateSettings,
    probe,
    start,
    cancel,
    qrcode,
    poll,
    onUpdate,
    namingVariables,
    previewNaming,
    probeMore,
    probeRange,
  };
})();
