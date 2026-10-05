// 下载请求与命名变量：解析页与内容库共用同一份实现。
//
// 命名规则是"一个条目叫什么"的唯一出处，两个入口必须算出同一个名字 ——
// 所以 batchNaming / singleNaming 只在这里实现一次，谁也别抄第二份。

import * as api from "./api";

export const KIND_LABELS = {
  video: "视频",
  fav: "收藏夹",
  collection: "合集",
  series: "系列",
  opus: "图文",
  article: "专栏",
  audio: "音频",
  space: "UP 空间",
  bangumi: "番剧",
  cheese: "课程",
};

export function localDate(unixSecs) {
  const d = unixSecs ? new Date(unixSecs * 1000) : new Date();
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function folderLevel(batch, entry) {
  if (batch.kind === "bangumi" || batch.kind === "cheese") return "";
  if (batch.kind === "opus" && batch.items.length <= 1) return "";
  if (batch.kind === "article") return "";
  if (batch.kind === "opus" || batch.kind === "audio") return KIND_LABELS[batch.kind] ?? "";
  // UP 空间：逐条判断这条视频属于哪个合集；不在任何合集（或没查到）的归「单独投稿」
  if (batch.kind === "space") return entry?.collection || "单独投稿";
  return batch.title;
}

export function singleNaming(probe) {
  return {
    part_title: probe.part_title || probe.title,
    part_index: probe.part_index || 1,
    aid: probe.aid || 0,
    owner_mid: probe.owner_mid || 0,
    series_title: "",
    episode_index: 0,
    episode_title: "",
    collection_title: "",
    source_kind: KIND_LABELS[probe.kind] ?? "视频",
    index: 0,
    date: localDate(),
    publish_date: localDate(probe.pubdate),
  };
}

/** 批量条目的命名变量：番剧/课程用剧集信息，其余用合集信息 + 列表序号 */
export function batchNaming(batch, entry, position) {
  const episode = batch.kind === "bangumi" || batch.kind === "cheese";
  return {
    part_title: "",
    part_index: 0,
    aid: 0,
    owner_mid: 0,
    series_title: episode ? batch.title : "",
    episode_index: episode ? position : 0,
    episode_title: episode ? entry.title : "",
    // 文件夹第二层（{collection_title}）按来源给：
    // - 合集/收藏夹/系列：来源标题就是合集名，用它
    // - UP 空间：本来就没有合集，留空（第一层已经是 UP 名，再放"…的投稿"是重复）
    // - 图文/音频：给类型层（图文 / 音频），既分开了内容类型又不重复 UP 名
    // - 单条图文/专栏：没有合集层，留空（否则标题在路径里出现两次）
    collection_title: folderLevel(batch, entry),
    // 空间解析时合集归属已在解析阶段定死（含"单独投稿"），下载端不再逐条补查。
    // 这个标志随请求落到 task.json，断点续传的恢复也不补查。
    collection_resolved: batch.kind === "space" && !!batch.collection_mapped,
    source_kind: KIND_LABELS[batch.kind] ?? "视频",
    // {index} 的补零宽度按本批条数算：20 条补到 2 位、几千条补到 4 位，
    // 这样目录按名称排序才是 01、02 … 10，而不是 1、10、2
    // 补零宽度按来源总数（拿不到总数时用本批条数）：表格与文件名同宽，
    // 而且来源以后继续拉长，已编号的位数也不会变
    index_pad: String(Math.max(batch.total || batch.items.length, 1)).length,
    // 序号由表格按"由旧到新"算好后传进来（row.abs），这里不再倒第二次。
    // 单条图文/专栏没有批次上下文，不给编号。
    // 图文/专栏：编号 = 在来源里的固定位置（从最新那头数，1 起）。
    // 列表只会在末尾追加，已加载条目的位置永远不变 —— 所以编号冻结、不会漂移，
    // 而且预览与落盘用的是同一个值（图文接口不给总数，没法像其它来源那样换算成"由旧到新"）。
    index: position,
    date: localDate(),
    publish_date: "",
  };
}

export function pickDefaultQuality(probe, settings) {
  const prefs = settings?.quality_prefs ?? [];
  for (const pref of prefs) {
    if (probe.qualities?.some((q) => q.qn === pref.qn && q.available)) return pref.qn;
  }
  // 只有**没自定义**优先顺序时，媒体页的「视频清晰度」才说话；
  // 表非空就按表走，表里的档位都不可用才退回推荐值（不去借单值，免得两套设置互相打架）
  if (!prefs.length) {
    const single = settings?.default_quality ?? 0;
    if (single > 0 && probe.qualities?.some((q) => q.qn === single && q.available)) return single;
  }
  return probe.recommended_quality;
}

/** 默认音轨：同样按优先顺序取第一条可用的 */
export function pickDefaultAudio(probe, settings) {
  const prefs = settings?.audio_prefs ?? [];
  for (const kind of prefs) {
    const hit = probe.audios?.find((a) => a.kind === kind);
    if (hit?.available) return kind;
  }
  // 同上：只有没自定义时，媒体页的「音频质量」才说话
  if (!prefs.length) {
    const single = settings?.default_audio ?? "auto";
    const hit = probe.audios?.find((a) => a.kind === single);
    if (hit?.available) return single;
  }
  return "normal";
}

/** 序号列的补零宽度与文件名一致：100 条补 3 位，避免表里 "01" 而磁盘上是 "001" */
export function paddedSeq(row) {
  const probe = row.source.probe;
  const width = String(Math.max(probe.total || probe.items.length, 1)).length;
  return String(row.abs).padStart(Math.max(width, 2), "0");
}

/** 单视频入队（单条链接的来源） */
export async function enqueueSingle(probe, options = {}) {
  return api.startDownload({
    bvid: probe.bvid,
    cid: probe.cid,
    title: probe.title,
    owner: probe.owner,
    source: "video",
    quality: options.quality ?? probe.recommended_quality,
    audio: options.audio ?? "normal",
    cover: probe.cover,
    naming: singleNaming(probe),
  });
}

/** 批量条目入队。`items` 里每条要带 bvid/cid/ep_id/opus_id/au_id/title；
 *  `absoluteOf(index)` 给出它在来源里的序号（由旧到新，1 起）。 */
export async function enqueueBatch(probe, items, options = {}) {
  let started = 0;
  for (const [index, entry] of items.entries()) {
    try {
      await api.startDownload({
        bvid: entry.bvid,
        cid: entry.cid,
        ep_id: entry.ep_id,
        opus_id: entry.opus_id ?? "",
        au_id: entry.au_id ?? "",
        source: probe.kind,
        title: entry.title,
        owner: probe.owner,
        quality: options.quality ?? 0,
        audio: options.audio ?? "normal",
        cover: "",
        naming: batchNaming(probe, entry, options.absoluteOf ? options.absoluteOf(index) : index + 1),
      });
      started += 1;
    } catch (error) {
      if (options.onError) options.onError(entry, error);
    }
  }
  return started;
}
