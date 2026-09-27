<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import * as api from "../api";
import StepHeader from "../components/StepHeader.vue";

const props = defineProps({
  login: { type: Object, required: true },
  settings: { type: Object, default: null },
});
const emit = defineEmits(["toast", "goto"]);

const mode = ref("batch");
const text = ref("");
const parsing = ref(false);
const done = ref(0);
const total = ref(0);
const items = ref([]);

const KIND_LABELS = {
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

/** 批量来源里每条的勾选状态 */
/** 本次解析被跳过的重复来源，列在解析结果里 */
const parseSkipped = ref([]);
const selected = ref(new Set());
const okItems = computed(() => items.value.filter((item) => item.ok));
const failedItems = computed(() => items.value.filter((item) => !item.ok));

/** 已经解析出来的来源条数：视频算 1，批量来源算已加载条数 */
function sourceCount(item) {
  return item.probe.kind === "video" ? 1 : item.probe.items.length;
}

function itemKey(item) {
  return item.probe ? `${item.probe.kind}:${item.bvid || `ep-${item.epId}`}` : item.input;
}
function hasKey(key) {
  return okItems.value.some((item) => itemKey(item) === key);
}

const steps = computed(() => {
  return [
    {
      index: 1,
      title: "解析来源",
      hint: parsing.value ? `解析中 ${done.value}/${total.value}` : "输入链接",
      // 状态跟着当前所在页面走：在选择页时第 1 步才算完成
      state: view.value === "select" ? "done" : "active",
      to: view.value === "select" ? "input" : "",
    },
    {
      index: 2,
      title: "选择内容",
      hint: view.value === "select"
        ? activeSource.value?.probe.kind === "video"
          ? "确认清晰度"
          : `已选 ${selectedCount.value} / 已加载 ${loadedCount.value}`
        : okItems.value.length
          ? `${okItems.value.length} 个来源待选择`
          : "等待解析",
      state: view.value === "select" ? "active" : "idle",
      // 有解析结果时第 2 步才可跳
      to: view.value !== "select" && items.value.length && !parsing.value ? "select" : "",
    },
  ];
});

// 支持的来源；图标用路径数据，避免为五个小图标各写一段模板
const sources = [
  {
    label: "视频与分P",
    icon: ["M4.6 6.4h14.8v11.2H4.6z", "m10.6 10.2 3.4 1.8-3.4 1.8z"],
  },
  {
    label: "收藏夹与合集",
    icon: ["M6.6 4.6h10.8v14.8l-5.4-3.6-5.4 3.6z"],
  },
  {
    label: "番剧与课程",
    // 屏幕 + 两侧支脚，支脚在 14px 下要画得够开才看得出来
    icon: ["M4.6 6.4h14.8v10.6H4.6z", "M9.8 20.4 8.8 17M14.2 20.4l1-3.4"],
  },
  {
    label: "图文与专栏",
    // 画面 + 右下角的正文线
    icon: ["M4.6 6.4h15v11.2h-15z", "M7.4 13.6h5.2M7.4 15.8h3.4"],
  },
  {
    label: "音频投稿",
    // 音符：符头 + 符干 + 旗
    icon: ["M10.2 16.4a1.9 1.9 0 1 1-3.8 0 1.9 1.9 0 0 1 3.8 0Z", "M10.2 16.4V7.6l6.4-1.6v8.8", "M16.6 14.8a1.9 1.9 0 1 1-3.8 0 1.9 1.9 0 0 1 3.8 0Z"],
  },
  {
    label: "UP 空间",
    icon: [
      "M12 5.4a3.2 3.2 0 1 1 0 6.4 3.2 3.2 0 0 1 0-6.4Z",
      "M5.8 19.4c.9-3 3.4-4.6 6.2-4.6s5.3 1.6 6.2 4.6",
    ],
  },
  {
    label: "多行批量",
    icon: ["M5.4 7.4h1.4M9.6 7.4h9M5.4 12h1.4M9.6 12h9M5.4 16.6h1.4M9.6 16.6h9"],
  },
];

function formatDuration(seconds) {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
    : `${m}:${String(s).padStart(2, "0")}`;
}

async function pasteFromClipboard() {
  const value = await api.readClipboard();
  if (!value) {
    emit("toast", "读取剪贴板失败，请手动粘贴");
    return;
  }
  text.value = text.value.trim() ? `${text.value.trim()}\n${value}` : value;
}

/** 优先用设置里的默认清晰度；该视频拿不到时退回推荐档 */
/** 默认清晰度：按设置里的优先顺序取第一条这视频真能拿到的档位 */
function pickDefaultQuality(probe) {
  const prefs = props.settings?.quality_prefs ?? [];
  for (const pref of prefs) {
    if (probe.qualities?.some((q) => q.qn === pref.qn && q.available)) return pref.qn;
  }
  return probe.recommended_quality;
}

/** 默认音轨：同样按优先顺序取第一条可用的 */
function pickDefaultAudio(probe) {
  const prefs = props.settings?.audio_prefs ?? [];
  for (const kind of prefs) {
    const hit = probe.audios?.find((a) => a.kind === kind);
    if (hit?.available) return kind;
  }
  return "normal";
}

async function parse() {
  // 同一链接贴两次不必解析两遍；被跳过的记下来，回头列在解析结果里
  const skipped = [];
  const inputs = [];
  const seenLine = new Set();
  for (const raw of text.value.split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    if (seenLine.has(line)) {
      skipped.push({ input: line, reason: "与前面的链接相同" });
      continue;
    }
    seenLine.add(line);
    inputs.push(line);
  }
  if (!inputs.length || parsing.value) return;

  // 按解析节奏分批：批内并发、批间等待、每 N 条休息，降低触发风控的概率
  parsing.value = true;
  items.value = [];
  selected.value = new Set();
  done.value = 0;
  total.value = inputs.length;

  const collected = [];
  const seenKeys = new Map();
  const batch = Math.max(1, props.settings?.parse_batch ?? 8);
  const batchWait = props.settings?.parse_batch_wait_ms ?? 1000;
  const restEvery = Math.max(1, props.settings?.parse_rest_every ?? 100);
  const restMs = props.settings?.parse_rest_ms ?? 3000;

  const probeOne = async (input) => {
    try {
      // 批量解析模式下，视频链接会去解析它所在的合集；单个视频模式只解析这一个
      const probe = await api.probeSource(input, mode.value === "batch");
      // 同一个来源（同一链接/同一合集的另一个视频）只保留第一次
      if (probe.key && seenKeys.has(probe.key)) {
        skipped.push({
          input,
          reason: `与「${seenKeys.get(probe.key)}」是同一个来源`,
        });
        done.value += 1;
        return;
      }
      if (probe.key) seenKeys.set(probe.key, probe.title || input);
      if (probe.kind === "video") {
        collected.push({
          input,
          ok: true,
          error: "",
          probe,
          quality: pickDefaultQuality(probe),
          audio: pickDefaultAudio(probe),
        });
      } else {
        // 先完成全部处理，最后才 push —— 中途出错不会留下半成品条目
        // 默认不勾选：清单可能很长，先让用户看清再挑
        collected.push({
          input,
          ok: true,
          error: "",
          probe,
          quality: pickDefaultQuality(probe),
          audio: pickDefaultAudio(probe),
        });
      }
    } catch (error) {
      collected.push({ input, ok: false, error: String(error) });
    }
    done.value += 1;
    items.value = [...collected];
  };

  for (let start = 0; start < inputs.length; start += batch) {
    await Promise.all(inputs.slice(start, start + batch).map(probeOne));
    const finished = start + batch;
    if (finished >= inputs.length) break;
    if (finished % restEvery === 0 && restMs > 0) {
      await new Promise((r) => setTimeout(r, restMs));
    } else if (batchWait > 0) {
      await new Promise((r) => setTimeout(r, batchWait));
    }
  }

  parsing.value = false;
  parseSkipped.value = [...skipped];

  const failed = collected.filter((item) => !item.ok).length;
  if (failed) {
    emit("toast", `${collected.length - failed} 条解析成功，${failed} 条失败`);
  }
  if (skipped.length) {
    emit("toast", `已跳过 ${skipped.length} 个重复来源，明细见解析结果`);
  }

  // 解析成功的一律进「选择内容」页：单视频与批量清单都在那里挑
  const firstOk = collected.find((item) => item.ok);
  if (firstOk) openSelect(firstOk.input);
}

function toggleItem(key) {
  const next = new Set(selected.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  selected.value = next;
}

function selectAll() {
  const next = new Set();
  okItems.value.forEach((item) => {
    if (item.probe.kind !== "video") {
      item.probe.items.forEach((entry) =>
        next.add(`${item.probe.kind}:${entry.bvid || `ep-${entry.ep_id}`}`)
      );
    }
  });
  selected.value = next;
}

function selectNone() {
  selected.value = new Set();
}

// ---------- 选择内容页（批量来源）----------
// 解析结果不再塞在输入页里，而是独立一页：表格 + 分批加载。
// 首次解析只给第一页，「继续解析」用后端缓存接着往后拉。
const view = ref("input"); // input | select
const batchInput = ref(""); // 当前查看的批量来源（继续解析的键）
const batchSize = ref(50); // 一次往后拉多少条
const loadingMore = ref(false);

/** 已解析出的来源：单视频与批量来源都能在选择页里看 */
const allSources = computed(() => okItems.value);

const activeSource = computed(() => {
  const list = allSources.value;
  return list.find((item) => item.input === batchInput.value) || list[0] || null;
});

/** 选择页要占满高度：顶部工具条与底部统计固定，只有表体滚动 */
const isSelectView = computed(() => view.value === "select" && !!activeSource.value);

/** 当前来源是批量清单（单视频没有表格与页码） */
const activeIsBatch = computed(() => !!activeSource.value && activeSource.value.probe.kind !== "video");


/** 表头：只有一个来源就显示它自己，多个来源显示来源数 */
const headerTitle = computed(() => {
  if (allSources.value.length === 1) return activeSource.value?.probe.title ?? "";
  if (videosOnly.value.length === allSources.value.length) {
    return `${allSources.value.length} 个单视频`;
  }
  return `${allSources.value.length} 个来源`;
});

const headerTag = computed(() => {
  if (allSources.value.length === 1) return kindLabel(activeSource.value?.probe.kind ?? "");
  return videosOnly.value.length === allSources.value.length ? "视频" : "";
});

/** 行悬停显示该行将落盘的文件名（与下载共用后端渲染器） */
function fileNameOf(row) {
  const name = fileNames.value[row.seq - 1];
  return name ? `${row.title}  |  文件名：${name}` : row.title;
}

function isSelected(row) {
  return selected.value.has(row.key);
}

function toggleEntry(row) {
  const next = new Set(selected.value);
  if (next.has(row.key)) next.delete(row.key);
  else next.add(row.key);
  selected.value = next;
}

const loadedCount = computed(() => tableRows.value.length);

/** 工具条计数：按序号加载时说明这批是来源里的哪一段，免得跟"已加载 1/581"混淆 */
const loadedHint = computed(() => {
  const probe = activeSource.value?.probe;
  if (!probe) return "";
  const total = probe.total || 0;
  const from = probe.from_index || 1;
  if (activeIsBatch.value && from > 1 && loadedCount.value > 0) {
    return `第 ${from}–${from + loadedCount.value - 1} 条 / 共 ${total} 条`;
  }
  // 图文接口不给总数，只报已加载
  if (!total) return `已加载 ${loadedCount.value} 项`;
  return `已加载 ${loadedCount.value} / ${total} 项`;
});

/** 这批之后还有没加载的（撞上限的来源靠「加载下一批」接着拉），下一个起点序号 */
const nextRangeFrom = computed(() => {
  const probe = activeSource.value?.probe;
  if (!probe || !activeIsBatch.value) return 1;
  return (probe.from_index || 1) + loadedCount.value;
});

/** 撞到单次上限、但来源后面还有内容：这时要把加载入口留着，否则剩下的永远拿不到。
 *  注意只看"是不是撞上限"——来源自己取完了（比如音频报 296 实取 266）不该再给续拉。 */
const moreRemain = computed(() => {
  const probe = activeSource.value?.probe;
  if (!probe || !activeIsBatch.value || !probe.capped) return false;
  const total = probe.total || 0;
  return !total || nextRangeFrom.value <= total;
});
const selectedCount = computed(() => tableRows.value.filter((row) => isSelected(row)).length);
const allLoadedSelected = computed(
  () => loadedCount.value > 0 && selectedCount.value === loadedCount.value
);

/** 全选/全不选只作用于"已加载"的部分，没拉下来的不会被选中 */
function toggleAllLoaded(checked) {
  const next = new Set(selected.value);
  for (const row of tableRows.value) {
    if (checked) next.add(row.key);
    else next.delete(row.key);
  }
  selected.value = next;
}

// ---- 下载设置弹层绑定：多视频清单用共享的一档，单来源用来源自己的 ----
const multiQuality = ref(0);
const multiAudio = ref("normal");

const dlQuality = computed({
  get: () => (useShared.value ? multiQuality.value : activeSource.value?.quality ?? 0),
  set: (value) => {
    if (useShared.value) multiQuality.value = value;
    else if (activeSource.value) activeSource.value.quality = value;
  },
});

const dlAudio = computed({
  get: () => (useShared.value ? multiAudio.value : activeSource.value?.audio ?? "normal"),
  set: (value) => {
    if (useShared.value) multiAudio.value = value;
    else if (activeSource.value) activeSource.value.audio = value;
  },
});

const dlQualities = computed(() =>
  useShared.value
    ? allSources.value[0]?.probe.qualities ?? []
    : activeSource.value?.probe.qualities ?? []
);

const dlAudios = computed(() =>
  useShared.value
    ? allSources.value[0]?.probe.audios ?? []
    : activeSource.value?.probe.audios ?? []
);

// 多个来源时，共享档位默认取第一个来源的推荐值
watch(allSources, () => {
  const first = allSources.value[0];
  if (!first) return;
  multiQuality.value = pickDefaultQuality(first.probe);
  multiAudio.value = pickDefaultAudio(first.probe);
});

/** 按来源把行分组，用来在表里画出分界：哪几行属于哪个来源 */
/** 来源条数之和与实际行数之差 = 被判重掉的内容数 */
const totalItemCount = computed(() =>
  allSources.value.reduce(
    (sum, source) => sum + (source.probe.kind === "video" ? 1 : source.probe.items.length),
    0
  )
);
const dedupedCount = computed(() => totalItemCount.value - tableRows.value.length);

const tableGroups = computed(() => {
  const groups = [];
  for (const row of tableRows.value) {
    const last = groups[groups.length - 1];
    if (last && last.source === row.source) last.rows.push(row);
    else groups.push({ source: row.source, rows: [row] });
  }
  // 内容被前面的来源覆盖完的来源不再显示分组行
  return groups.filter((group) => group.rows.length > 0);
});

/** 折叠的分组（按来源输入记）：点分组行收起/展开该来源的行 */
const collapsed = ref(new Set());

function isCollapsed(input) {
  return collapsed.value.has(input);
}

/** 分组勾选：全选只覆盖这一组，不影响别的来源 */
function groupChecked(group) {
  return group.rows.length > 0 && group.rows.every((row) => isSelected(row));
}

function groupPartial(group) {
  const picked = group.rows.filter((row) => isSelected(row)).length;
  return picked > 0 && picked < group.rows.length;
}

function toggleGroupSelect(group, checked) {
  const next = new Set(selected.value);
  for (const row of group.rows) {
    if (checked) next.add(row.key);
    else next.delete(row.key);
  }
  selected.value = next;
}

function toggleGroup(input) {
  const next = new Set(collapsed.value);
  if (next.has(input)) next.delete(input);
  else next.add(input);
  collapsed.value = next;
}

/** 只有一个来源时不必分组（表头已经写了它是谁） */
const showGroups = computed(() => allSources.value.length > 1);

/** 有内容可列就出表（现在单视频也是表里的一行） */
const hasTable = computed(() => tableRows.value.length > 0);

function openSelect(input) {
  batchInput.value = input;
  view.value = "select";
}

/** 条目在来源内的唯一标识：视频用 bvid，番剧/课程用 ep_id，图文用 opus_id。
 *  注意不能用 `||` 串起来——ep_id 为空时 `ep-undefined` 也是真值，整列会撞成一个键。 */
function entryIdentity(entry) {
  if (entry.bvid) return entry.bvid;
  if (entry.ep_id) return `ep-${entry.ep_id}`;
  if (entry.opus_id) return `opus-${entry.opus_id}`;
  if (entry.au_id) return `au-${entry.au_id}`;
  return "unknown";
}

function entryKey(probe, entry) {
  return `${probe.kind}:${entryIdentity(entry)}`;
}

const videosOnly = computed(() => okItems.value.filter((item) => item.probe.kind === "video"));
/** 多个来源时清晰度/音轨共用一个档位 */
const useShared = computed(() => allSources.value.length > 1);

/**
 * 所有来源都摊成同一张表：批量来源出它的每条内容，单视频出它自己这一行。
 * 合集与单视频混着贴也是同一张表，不再按来源切来切去。
 *
 * `seq` 是它在表里的序号（给人看），`abs` 是它在来源里的真实序号（给序号列与命名模板的
 * {index} 用）——按序号加载时这批从第 301 条开始，abs 就从 301 算起，文件名不会与上一批撞。
 * 单视频没有"第几条"的概念，固定 1。
 */
const tableRows = computed(() => {
  const rows = [];
  // 内容级判重：同一个视频可能被多个来源覆盖（合集与其中的单个视频、
  // UP 空间与它的一条投稿），只保留先出现的那个
  const seenItems = new Set();
  const contentKey = (entry, source) =>
    entry ? `item:${entryIdentity(entry)}` : `item:${source.probe.bvid}`;
  for (const source of allSources.value) {
    // 按序号加载时这批的第一条不是来源里的第 1 条；abs 记来源内的真实位置
    const fromIndex = source.probe.from_index || 1;
    if (source.probe.kind === "video") {
      const ck = contentKey(null, source);
      if (seenItems.has(ck)) continue;
      seenItems.add(ck);
      rows.push({
        key: source.input,
        seq: rows.length + 1,
        index: 0,
        abs: fromIndex,
        title: source.probe.title,
        owner: source.probe.owner,
        duration: source.probe.duration,
        source,
      });
      continue;
    }
    source.probe.items.forEach((entry, position) => {
      const ck = contentKey(entry, source);
      if (seenItems.has(ck)) return;
      seenItems.add(ck);
      rows.push({
        key: entryKey(source.probe, entry),
        seq: rows.length + 1,
        index: position + 1,
        abs: fromIndex + position,
        title: entry.title,
        owner: entry.owner,
        duration: entry.duration,
        entry,
        source,
      });
    });
  }
  return rows;
});

/** 行悬停时显示该行将落盘的文件名：由后端用与下载同一个渲染器算出 */
const fileNames = ref([]);
let nameSeq = 0;

async function refreshNames() {
  const rows = tableRows.value;
  const seq = ++nameSeq;
  if (!rows.length) {
    fileNames.value = [];
    return;
  }
  try {
    const names = await api.previewNames(
      rows.map((row) => ({
        title: row.title,
        owner: row.owner || row.source.probe.owner || "",
        bvid: row.entry ? row.entry.bvid : row.source.probe.bvid,
        cid: row.entry ? row.entry.cid : row.source.probe.cid,
        kind: row.source.probe.kind,
        naming: row.entry
          ? batchNaming(row.source.probe, row.entry, row.abs)
          : singleNaming(row.source.probe),
      })),
      useShared.value ? multiQuality.value : activeSource.value?.quality ?? 0,
      localDate(),
      props.settings?.container === "mkv" ? "mkv" : "mp4"
    );
    if (seq === nameSeq) fileNames.value = names;
  } catch {
    if (seq === nameSeq) fileNames.value = [];
  }
}

watch(
  () => [tableRows.value.length, props.settings?.naming_template, props.settings?.container],
  refreshNames,
  { immediate: true }
);

async function loadMore() {
  const source = activeSource.value;
  if (!source || loadingMore.value || source.probe.exhausted) return;
  loadingMore.value = true;
  try {
    const more = await api.probeMore(source.input, batchSize.value);
    source.probe.items.push(...more.items);
    source.probe.loaded = more.loaded;
    source.probe.total = more.total;
    source.probe.exhausted = more.exhausted;
    source.probe.capped = more.capped;
    source.probe.note = more.note;
  } catch (error) {
    emit("toast", String(error));
  } finally {
    loadingMore.value = false;
  }
}

/** 把给定行逐个入队；序号从 1 开始，供命名模板的 {index} / {episode_index} 用 */
async function startRows(rows) {
  if (!rows.length) return 0;
  let started = 0;
  for (const row of rows) {
    try {
      if (!row.entry) {
        // 单视频行：每一个都是独立来源，多个来源时用共享的清晰度/音轨
        await api.startDownload({
          bvid: row.source.probe.bvid,
          cid: row.source.probe.cid,
          title: row.source.probe.title,
          owner: row.source.probe.owner,
          source: "video",
          quality: multiQuality.value,
          audio: multiAudio.value,
          cover: row.source.probe.cover,
          naming: singleNaming(row.source.probe),
        });
      } else {
        await api.startDownload({
          bvid: row.entry.bvid,
          cid: row.entry.cid,
          ep_id: row.entry.ep_id,
          opus_id: row.entry.opus_id ?? "",
          au_id: row.entry.au_id ?? "",
          source: row.source.probe.kind,
          title: row.entry.title,
          owner: row.source.probe.owner,
          quality: row.source.quality,
          audio: row.source.audio,
          cover: "",
          naming: batchNaming(row.source.probe, row.entry, row.abs),
        });
      }
      started += 1;
    } catch (error) {
      emit("toast", `${row.title}: ${error}`);
    }
  }
  if (started) {
    emit("toast", `已加入 ${started} 个下载任务`);
    emit("goto", "transfer");
  }
  return started;
}

/** 下载：清单里勾选的行；只有一个视频时直接下这一条 */
async function downloadSelected() {
  if (!hasTable.value) {
    if (activeSource.value) await startSingle(activeSource.value);
    return;
  }
  const picked = tableRows.value.filter((row) => isSelected(row));
  if (!picked.length) {
    emit("toast", "先勾选要下载的内容");
    return;
  }
  await startRows(picked);
}

/** 下载全部：不看勾选，把已加载的行全部入队 */
async function downloadAll() {
  if (!hasTable.value) {
    if (activeSource.value) await startSingle(activeSource.value);
    return;
  }
  await startRows([...tableRows.value]);
}

// 「下载设置」弹层（清晰度/音轨）
const pickingDl = ref(false);
const dlPanel = ref(null);

// 工具条上的窄版「每批 N」：自定义菜单，原生 select 的去不掉的箭头会占宽
const BATCH_SIZES = [20, 50, 100, 200];
const pickingBatch = ref(false);
const batchPanel = ref(null);

// 「解析」下拉：一次拉一批之外的两种批量做法
const pickingParse = ref(false);
const parsePanel = ref(null);
const parsingAll = ref(false);

/** 一直往后拉，直到来源到底或到单次上限；每拉完一页刷新计数 */
async function loadAll() {
  const source = activeSource.value;
  if (!source || parsingAll.value) return;
  parsingAll.value = true;
  try {
    for (let guard = 0; guard < 200 && !source.probe.exhausted; guard += 1) {
      const before = source.probe.loaded;
      await loadMore();
      // 没有进展（出错、被限流）就停，别在原地打转
      if (source.probe.loaded === before) break;
    }
  } finally {
    parsingAll.value = false;
  }
}

async function parseAll() {
  pickingParse.value = false;
  await loadAll();
}

/** 后台解析全部并下载：把余下的分页拉完，再把全部条目排进下载队列 */
async function parseAllAndDownload() {
  pickingParse.value = false;
  await loadAll();
  await downloadAll();
}

// 「按序号加载」：超过单次上限的来源靠它分批拉完（第 1–300、301–600……互不重叠）
const pickingRange = ref(false);
const rangeFrom = ref(1);
const rangeLoading = ref(false);

/** 打开输入框时把序号预填成"接着已加载的往后" */
function openRange() {
  rangeFrom.value = nextRangeFrom.value;
  pickingRange.value = true;
}

/** 「加载下一批」：直接接着这批往后取，不用手输序号 */
async function loadNextRange() {
  rangeFrom.value = nextRangeFrom.value;
  await loadRange();
}

async function loadRange() {
  const source = activeSource.value;
  const from = Math.max(1, Math.floor(Number(rangeFrom.value) || 1));
  if (!source || rangeLoading.value) return;
  rangeLoading.value = true;
  try {
    source.probe = await api.probeRange(source.input, from);
    // 换了一批：勾选与折叠状态跟着清掉，序号列也从新的起点算
    selected.value = new Set();
    collapsed.value = new Set();
    pickingParse.value = false;
    pickingRange.value = false;
    emit(
      "toast",
      `已加载第 ${source.probe.from_index}–${source.probe.from_index + source.probe.loaded - 1} 条`
    );
  } catch (error) {
    emit("toast", String(error));
  } finally {
    rangeLoading.value = false;
  }
}

function chooseBatchSize(size) {
  batchSize.value = size;
  pickingBatch.value = false;
}

function onDlDocumentDown(event) {
  if (pickingDl.value && dlPanel.value && !dlPanel.value.contains(event.target)) {
    pickingDl.value = false;
  }
  if (pickingParse.value && parsePanel.value && !parsePanel.value.contains(event.target)) {
    pickingParse.value = false;
  }
  if (pickingBatch.value && batchPanel.value && !batchPanel.value.contains(event.target)) {
    pickingBatch.value = false;
  }
}

function onDlKeydown(event) {
  if (event.key === "Escape") {
    pickingDl.value = false;
    pickingBatch.value = false;
  }
}

onMounted(() => {
  document.addEventListener("mousedown", onDlDocumentDown);
  document.addEventListener("keydown", onDlKeydown);
});

onUnmounted(() => {
  document.removeEventListener("mousedown", onDlDocumentDown);
  document.removeEventListener("keydown", onDlKeydown);
});

/** 步骤条点击跳转 */
function gotoStep(target) {
  if (target === "input") {
    view.value = "input";
    return;
  }
  if (target === "select" && okItems.value.length) {
    openSelect(batchInput.value || okItems.value[0].input);
  }
}

/** 清空全部解析结果 */
function resetParsed() {
  items.value = [];
  parseSkipped.value = [];
  selected.value = new Set();
  text.value = "";
  view.value = "input";
}

function removeItem(input) {
  const index = items.value.findIndex((item) => item.input === input);
  if (index >= 0) items.value.splice(index, 1);
}

/** 来源类型的中文标签（复用文件开头那份，video 也在里面） */
function kindLabel(kind) {
  return KIND_LABELS[kind] || kind;
}

/** 本地时区的 YYYY-MM-DD；不带参数即今天 */
function localDate(unixSecs) {
  const d = unixSecs ? new Date(unixSecs * 1000) : new Date();
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** 单视频的命名变量：分 P 信息来自解析结果，批量专属的留空 */
function singleNaming(probe) {
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
function batchNaming(batch, entry, position) {
  const episode = batch.kind === "bangumi" || batch.kind === "cheese";
  return {
    part_title: "",
    part_index: 0,
    aid: 0,
    owner_mid: 0,
    series_title: episode ? batch.title : "",
    episode_index: episode ? position : 0,
    episode_title: episode ? entry.title : "",
    collection_title: episode ? "" : batch.title,
    source_kind: KIND_LABELS[batch.kind] ?? "视频",
    index: position,
    date: localDate(),
    publish_date: "",
  };
}

async function startSingle(item) {
  try {
    await api.startDownload({
      bvid: item.probe.bvid,
      cid: item.probe.cid,
      title: item.probe.title,
      owner: item.probe.owner,
      source: "video",
      quality: item.quality ?? item.probe.recommended_quality,
      audio: item.audio ?? "normal",
      cover: item.probe.cover,
      naming: singleNaming(item.probe),
    });
    return true;
  } catch (error) {
    emit("toast", String(error));
    return false;
  }
}

</script>

<template>
  <div class="parse-page" :class="{ 'fill-height': isSelectView }">
    <StepHeader :steps="steps" @select="gotoStep" />

    <!-- 选择内容：解析后的独立一页 -->
    <section v-if="view === 'select' && activeSource" class="card select-page">
      <header class="select-bar">
        <button class="back" title="返回解析" @click="view = 'input'">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path
              d="M19 12H5.6M11 5.6 4.6 12l6.4 6.4"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </button>
        <h2 class="select-title" :title="headerTitle">{{ headerTitle }}</h2>
        <span v-if="headerTag" class="kind-tag">{{ headerTag }}</span>

        <!-- 计数与动作成组右对齐：窄窗口整体换行后仍然贴右（标签则留在左） -->
        <div class="bar-right">
        <template v-if="useShared">
          <span class="loaded-hint num">共 {{ loadedCount }} 条</span>
        </template>
        <template v-else-if="activeIsBatch">
          <span class="loaded-hint num">{{ loadedHint }}</span>
          <!-- 拉到底就不再给加载控件：只留计数与下载动作（照着「解析完全」的样子） -->
          <template v-if="!activeSource.probe.exhausted">
            <div ref="batchPanel" class="batch-picker">
              <button
                class="ghost compact"
                :class="{ on: pickingBatch }"
                aria-haspopup="menu"
                @click="pickingBatch = !pickingBatch"
              >
                每批 {{ batchSize }}
                <svg class="caret" viewBox="0 0 24 24" aria-hidden="true">
                  <path
                    d="m6 9.5 6 6 6-6"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                </svg>
              </button>
              <Transition name="picker">
                <div v-if="pickingBatch" class="batch-pop pop-in">
                  <button
                    v-for="n in BATCH_SIZES"
                    :key="n"
                    class="batch-item"
                    :class="{ on: n === batchSize }"
                    @click="chooseBatchSize(n)"
                  >
                    每批 {{ n }}
                  </button>
                </div>
              </Transition>
            </div>

            <!-- 一个按钮：点文字拉一批，点右侧箭头选解析方式（箭头在按钮内） -->
            <div ref="parsePanel" class="parse-split" :class="{ on: pickingParse }">
              <button class="seg main" :disabled="loadingMore" @click="loadMore">
                {{ loadingMore ? `解析中 ${loadedCount} / ${activeSource.probe.total}` : "继续解析" }}
              </button>
              <button
                class="seg arrow"
                :disabled="loadingMore"
                aria-haspopup="menu"
                title="更多解析方式"
                @click="pickingParse = !pickingParse"
              >
                <svg class="caret" viewBox="0 0 24 24" aria-hidden="true">
                  <path
                    d="m6 9.5 6 6 6-6"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                </svg>
              </button>
              <Transition name="picker">
                <div v-if="pickingParse" class="parse-pop pop-in">
                  <button class="parse-item" @click="parseAll">
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path
                        d="M3.6 6.6 5.2 8.2l3-3.6M3.6 12.2 5.2 13.8l3-3.6M3.6 17.8 5.2 19.4l3-3.6M12 7.2h8.4M12 12.8h8.4M12 18.4h8.4"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.7"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      />
                    </svg>
                    解析全部
                  </button>
                  <button class="parse-item" @click="parseAllAndDownload">
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path
                        d="M12 4v9.6m0 0L8.4 10m3.6 3.6L15.6 10M4.5 18.6h15"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.7"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      />
                    </svg>
                    后台解析全部并下载
                  </button>
                  <!-- 超过单次上限的来源：从这里按序号取下一批，两批不重叠 -->
                  <button
                    v-if="!pickingRange"
                    class="parse-item"
                    @click="openRange"
                  >
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path
                        d="M4.5 7.4h9M4.5 12h6M4.5 16.6h9M17 9.6V6m0 3.6L15.2 7.8M17 9.6l1.8-1.8M17 14.4v3.6m0-3.6 1.8 1.8M17 18l-1.8-1.8"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.7"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      />
                    </svg>
                    按序号加载…
                  </button>
                  <form v-else class="parse-range" @submit.prevent="loadRange">
                    <span>从第</span>
                    <input
                      v-model.number="rangeFrom"
                      type="number"
                      min="1"
                      :max="activeSource.probe.total"
                      class="num"
                    />
                    <span>条开始</span>
                    <button class="ghost small" type="submit" :disabled="rangeLoading">
                      {{ rangeLoading ? "加载中…" : "加载" }}
                    </button>
                  </form>
                </div>
              </Transition>
            </div>
          </template>

          <!-- 撞到单次上限、后面还有内容：留着入口按序号接着拿，否则剩下的永远拉不到 -->
          <template v-else-if="moreRemain">
            <div ref="parsePanel" class="parse-split" :class="{ on: pickingParse }">
              <button class="seg main" :disabled="rangeLoading" @click="loadNextRange">
                {{ rangeLoading ? "加载中…" : "加载下一批" }}
              </button>
              <button
                class="seg arrow"
                :disabled="rangeLoading"
                aria-haspopup="menu"
                title="按序号加载"
                @click="pickingParse = !pickingParse"
              >
                <svg class="caret" viewBox="0 0 24 24" aria-hidden="true">
                  <path
                    d="m6 9.5 6 6 6-6"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                </svg>
              </button>
              <Transition name="picker">
                <div v-if="pickingParse" class="parse-pop pop-in">
                  <button v-if="!pickingRange" class="parse-item" @click="openRange">
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path
                        d="M4.5 7.4h9M4.5 12h6M4.5 16.6h9M17 9.6V6m0 3.6L15.2 7.8M17 9.6l1.8-1.8M17 14.4v3.6m0-3.6 1.8 1.8M17 18l-1.8-1.8"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.7"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      />
                    </svg>
                    按序号加载…
                  </button>
                  <form v-else class="parse-range" @submit.prevent="loadRange">
                    <span>从第</span>
                    <input
                      v-model.number="rangeFrom"
                      type="number"
                      min="1"
                      :max="activeSource.probe.total"
                      class="num"
                    />
                    <span>条开始</span>
                    <button class="ghost small" type="submit" :disabled="rangeLoading">
                      {{ rangeLoading ? "加载中…" : "加载" }}
                    </button>
                  </form>
                </div>
              </Transition>
            </div>
          </template>
        </template>

        <!-- 还没拉完先不给"下载全部"：否则点下去只下载了已加载的那一部分 -->
        <button
          v-if="hasTable && (!activeIsBatch || activeSource.probe.exhausted)"
          class="ghost"
          @click="downloadAll"
        >
          下载全部
        </button>

        <!-- 清晰度/音轨收进弹层，工具条只留动作 -->
        <div ref="dlPanel" class="dl-settings">
          <button class="ghost" :class="{ on: pickingDl }" @click="pickingDl = !pickingDl">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M5 7.4h14M5 12h14M5 16.6h14"
                fill="none"
                stroke="currentColor"
                stroke-width="1.7"
                stroke-linecap="round"
              />
              <circle cx="9" cy="7.4" r="1.9" fill="var(--card)" stroke="currentColor" stroke-width="1.5" />
              <circle cx="15" cy="16.6" r="1.9" fill="var(--card)" stroke="currentColor" stroke-width="1.5" />
            </svg>
            下载设置
          </button>

          <Transition name="picker">
            <div v-if="pickingDl" class="dl-pop pop-in">
              <label class="pop-field">
                <span>清晰度</span>
                <select v-model.number="dlQuality">
                  <option
                    v-for="quality in dlQualities"
                    :key="quality.qn"
                    :value="quality.qn"
                    :disabled="!quality.available"
                  >
                    {{ quality.label }}{{ quality.hint ? `（${quality.hint}）` : "" }}
                  </option>
                </select>
              </label>
              <label class="pop-field">
                <span>音轨</span>
                <select v-model="dlAudio">
                  <option
                    v-for="audio in dlAudios"
                    :key="audio.kind"
                    :value="audio.kind"
                    :disabled="!audio.available"
                  >
                    {{ audio.label }}{{ audio.available ? "" : "（不可用）" }}
                  </option>
                </select>
              </label>
              <p class="pop-note">
                只对当前来源生效；设置里的「画质优先顺序」是全局的尝试顺序。
              </p>
            </div>
          </Transition>
        </div>

        <button
          class="primary"
          :disabled="hasTable && !selectedCount"
          @click="downloadSelected"
        >
          {{ hasTable ? `下载所选 (${selectedCount})` : "加入下载" }}
        </button>
        </div>
      </header>

      <p v-if="activeSource.probe.note" class="note">{{ activeSource.probe.note }}</p>

      <div v-if="hasTable" class="table-scroll">
        <table class="batch-table">
          <thead>
            <tr>
              <th class="col-check">
                <input
                  type="checkbox"
                  :checked="allLoadedSelected"
                  title="全选已加载"
                  @change="toggleAllLoaded($event.target.checked)"
                />
              </th>
              <th class="col-idx">序号</th>
              <th>标题</th>
              <th class="col-owner">UP 主</th>
              <th class="col-dur">时长</th>
            </tr>
          </thead>
          <tbody>
            <template v-for="group in tableGroups" :key="group.source.input">
              <tr
                v-if="showGroups"
                class="group-row"
                :class="{ folded: isCollapsed(group.source.input) }"
                :title="isCollapsed(group.source.input) ? '展开这个来源' : '收起这个来源'"
                @click="toggleGroup(group.source.input)"
              >
                <td colspan="5">
                  <input
                    type="checkbox"
                    class="group-check"
                    :checked="groupChecked(group)"
                    :indeterminate.prop="groupPartial(group)"
                    :title="`只选择「${group.source.probe.title}」`"
                    @click.stop
                    @change="toggleGroupSelect(group, $event.target.checked)"
                  />
                  <svg class="fold-arrow" viewBox="0 0 24 24" aria-hidden="true">
                    <path
                      d="m9 6 6 6-6 6"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    />
                  </svg>
                  <span class="group-kind">{{ kindLabel(group.source.probe.kind) }}</span>
                  <span class="group-title" :title="group.source.probe.title">
                    {{ group.source.probe.title }}
                  </span>
                  <span class="num faint">{{ group.rows.length }} 条</span>
                </td>
              </tr>
              <tr
                v-for="row in group.rows"
                v-show="!isCollapsed(group.source.input)"
                :key="row.key"
                :class="{ on: isSelected(row) }"
                :title="fileNameOf(row)"
              >
                <td class="col-check">
                  <input type="checkbox" :checked="isSelected(row)" @change="toggleEntry(row)" />
                </td>
                <td class="col-idx num">{{ String(row.abs).padStart(2, "0") }}</td>
                <td class="col-title">{{ row.title }}</td>
                <td class="col-owner" :title="row.owner">{{ row.owner || "—" }}</td>
                <td class="col-dur num">
                  {{
                    row.entry && row.entry.opus_id
                      ? row.source.probe.kind === "article"
                        ? "专栏"
                        : "图文"
                      : formatDuration(row.duration)
                  }}
                </td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>

      <!-- 底部统计只对批量清单有意义，单视频不渲染这条空栏 -->
      <footer v-if="hasTable" class="select-foot">
        <span class="foot-count">已选 <b class="num">{{ selectedCount }}</b> 项</span>
        <span class="foot-count">共 <b class="num">{{ loadedCount }}</b> 项</span>
        <span v-if="dedupedCount > 0" class="foot-count faint">
          （已去重 <b class="num">{{ dedupedCount }}</b> 条）
        </span>
        <span class="spacer"></span>
        <button class="mini" @click="toggleAllLoaded(true)">全选已加载</button>
      </footer>
    </section>

    <template v-else>

    <section class="card">
      <h1>解析链接</h1>
      <p class="lead">粘贴一个或多个 Bilibili 来源，解析后再选择要下载的内容与清晰度。</p>

      <div class="tabs" role="tablist">
        <button :class="{ active: mode === 'batch' }" role="tab" @click="mode = 'batch'">
          批量解析
        </button>
        <button :class="{ active: mode === 'single' }" role="tab" @click="mode = 'single'">
          单个链接
        </button>
      </div>

      <textarea
        v-if="mode === 'batch'"
        v-model="text"
        rows="7"
        spellcheck="false"
        placeholder="https://www.bilibili.com/video/BV1Vkag6TExf&#10;https://space.bilibili.com/xxx/favlist?fid=xxx&#10;https://space.bilibili.com/xxx/lists/xxx&#10;https://space.bilibili.com/xxx&#10;https://www.bilibili.com/bangumi/play/ssxxx"
        @keydown.ctrl.enter="parse"
        @keydown.meta.enter="parse"
      ></textarea>
      <input
        v-else
        v-model="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="粘贴视频链接、BV 号或 av 号"
        @keydown.enter="parse"
      />

      <p class="hint">
        每行一个来源；合集、收藏夹、系列、UP 空间、图文与音频都会按页加载，单条图文/专栏链接直接解析。
      </p>

      <div class="actions">
        <button class="ghost" @click="pasteFromClipboard">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <rect
              x="8.4"
              y="4.4"
              width="11.2"
              height="14.4"
              rx="2.2"
              fill="none"
              stroke="currentColor"
              stroke-width="1.6"
            />
            <path
              d="M15.6 4.4v2.2M6.6 7.4v10.4a2 2 0 0 0 2 2h6"
              fill="none"
              stroke="currentColor"
              stroke-width="1.6"
              stroke-linecap="round"
            />
          </svg>
          粘贴链接
        </button>
        <span class="spacer"></span>
        <span class="kbd">Ctrl + Enter</span>
        <button class="primary" :disabled="parsing || !text.trim()" @click="parse">
          {{ parsing ? `解析中 ${done}/${total}` : "开始解析" }}
        </button>
      </div>

      <div v-if="mode === 'batch'" class="sources">
        <span class="sources-label">支持来源</span>
        <span v-for="source in sources" :key="source.label" class="source">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path
              v-for="(d, index) in source.icon"
              :key="index"
              :d="d"
              fill="none"
              stroke="currentColor"
              stroke-width="1.6"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
          {{ source.label }}
        </span>
      </div>
    </section>

    <!-- 解析结果入口：成功的一律进「选择内容」页，这里只留入口与失败项 -->
    <section v-if="items.length" class="card results page-in">
      <header class="results-head">
        <h2>解析结果</h2>
        <span class="count num">{{ okItems.length }}</span>
        <span class="spacer"></span>
        <button class="ghost" @click="resetParsed">清空</button>
      </header>

      <div v-if="okItems.length" class="parsed-bar">
        <button
          v-for="item in okItems"
          :key="item.input"
          class="parsed-chip"
          :title="item.probe.title || item.input"
          @click="openSelect(item.input)"
        >
          <span class="kind">{{ KIND_LABELS[item.probe.kind] }}</span>
          <span class="chip-title">{{ item.probe.title || item.input }}</span>
          <span class="num faint">{{ sourceCount(item) }}</span>
        </button>
      </div>

      <div v-if="parseSkipped.length || dedupedCount > 0" class="skipped-box">
        <p class="skipped-head">
          去重结果
          <span class="num faint">
            {{ parseSkipped.length ? `跳过 ${parseSkipped.length} 个重复来源` : "" }}
            {{ parseSkipped.length && dedupedCount ? " · " : "" }}
            {{ dedupedCount ? `合并 ${dedupedCount} 条重复内容` : "" }}
          </span>
          <span v-if="parseSkipped.length > 5" class="skipped-more faint">（列表内可滚动）</span>
        </p>
        <ul class="skipped-list">
          <li v-for="(item, index) in parseSkipped" :key="`${item.input}-${index}`">
            <span class="skipped-input" :title="item.input">{{ item.input }}</span>
            <span class="skipped-reason">{{ item.reason }}</span>
          </li>
          <li v-if="dedupedCount > 0" class="skipped-merged">
            <span class="skipped-reason">
              另有 {{ dedupedCount }} 条内容与已有来源重复，已合并（不重复下载）
            </span>
          </li>
        </ul>
      </div>

      <ul v-if="failedItems.length" class="list">
        <li v-for="item in failedItems" :key="item.input" class="item failed">
          <div class="meta">
            <p class="title">{{ item.input }}</p>
            <p class="error">{{ item.error }}</p>
          </div>
          <button class="remove" title="移除" @click="removeItem(item.input)">✕</button>
        </li>
      </ul>

      <p v-if="!login.logged_in && okItems.length" class="login-tip">
        未登录最高只能下载 480P，点击右上角完成登录可解锁 1080P 及以上。
      </p>
    </section>
    </template>
  </div>
</template>

<style scoped>
.card {
  padding: 20px 22px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius-lg);
}

h1 {
  margin: 0;
  font-size: 25px;
  font-weight: 700;
  letter-spacing: -0.2px;
}

.lead {
  margin: 7px 0 0;
  font-size: 13px;
  color: var(--muted);
}

.tabs {
  display: flex;
  gap: 8px;
  margin: 12px 0 12px;
}

.tabs button {
  padding: 6px 15px;
  font-size: 12.5px;
  color: var(--muted);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: 999px;
  transition: all 0.15s ease;
}

.tabs button:hover {
  color: var(--text);
  border-color: var(--line);
}

.tabs button.active {
  color: var(--accent-dark);
  background: var(--accent-soft);
  border-color: var(--accent-line);
  font-weight: 600;
}

/* 勾选框不算"输入框"：这条规则里的 width:100% 会把它拉成整行宽 */
textarea,
input:not([type="checkbox"]) {
  width: 100%;
  padding: 12px 14px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  resize: vertical;
  line-height: 1.7;
  transition: border-color 0.15s ease;
}

textarea::placeholder,
input::placeholder {
  color: var(--faint);
}

textarea:focus,
input:focus {
  outline: none;
  border-color: var(--accent-line);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.hint {
  margin: 9px 0 0;
  font-size: 12px;
  color: var(--faint);
}

.actions {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 16px;
}

.spacer {
  flex: 1;
}

.kbd {
  font-size: 11.5px;
  color: var(--faint);
}

.primary {
  padding: 9px 22px;
  font-weight: 600;
  color: #fff;
  background: var(--accent);
  border-radius: var(--radius-sm);
  transition: background 0.15s ease;
}

.primary:hover:not(:disabled) {
  background: var(--accent-dark);
}

.primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.ghost {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 8px 14px;
  font-size: 12.5px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.ghost:hover {
  border-color: var(--accent-line);
  background: var(--raised);
}

.ghost svg {
  width: 15px;
  height: 15px;
  color: var(--muted);
}

.sources {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 16px;
  margin-top: 14px;
  padding-top: 13px;
  border-top: 1px solid var(--line-soft);
  font-size: 12px;
  color: var(--muted);
}

.sources-label {
  font-weight: 600;
  color: var(--text);
}

.source {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.source svg {
  flex: none;
  width: 14px;
  height: 14px;
  color: var(--accent);
}

/* 解析结果里的去重明细 */
.skipped-box {
  margin-top: 12px;
  padding: 10px 12px;
  background: var(--raised);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius-sm);
}

.skipped-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin: 0;
  font-size: 12.5px;
  font-weight: 600;
}

/* 明细不撑高整页：超过几行就在框内滚，整页该滚动的距离不受它影响 */
.skipped-list {
  margin: 7px 0 0;
  padding: 0;
  list-style: none;
  max-height: 108px;
  overflow-y: auto;
}

.skipped-list li {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 3px 0;
  font-size: 12px;
  color: var(--muted);
}

.skipped-input {
  flex: 0 1 auto;
  min-width: 0;
  max-width: 58%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  color: var(--text);
}

.skipped-reason {
  flex: 1;
  min-width: 0;
  color: var(--faint);
  overflow-wrap: anywhere;
}

/* 输入页里的"已解析来源"入口 */
.parsed-bar {
  display: flex;
  /* 与上面的标题行贴合：标题行自身 38px 高，再加外边距就显得空 */
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 0;
}

.parsed-chip {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  max-width: 100%;
  padding: 7px 12px;
  font-size: 12.5px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.parsed-chip:hover {
  border-color: var(--accent-line);
  background: var(--raised);
}

.parsed-chip .chip-title {
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 「下载设置」弹层：清晰度/音轨，工具条只留动作按钮 */
.dl-settings {
  position: relative;
  flex: none;
}

.dl-settings .ghost {
  gap: 7px;
  padding: 6px 11px;
  font-size: 12.5px;
}

.dl-settings .ghost svg {
  width: 15px;
  height: 15px;
}

.dl-settings .ghost.on {
  color: var(--accent);
  border-color: var(--accent-line);
  background: var(--raised);
}

.dl-pop {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 15;
  width: min(286px, calc(100vw - 60px));
  padding: 12px;
  text-align: left;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  box-shadow: 0 12px 30px rgba(20, 12, 16, 0.24);
}

.pop-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.pop-field + .pop-field {
  margin-top: 10px;
}

.pop-field span {
  font-size: 11.5px;
  color: var(--faint);
}

.pop-field select {
  width: 100%;
  height: 30px;
  padding: 0 9px;
  font-size: 12.5px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.pop-note {
  margin: 10px 0 0;
  font-size: 11px;
  line-height: 1.5;
  color: var(--faint);
}

.video-detail .file-line {
  align-items: flex-start;
}

/* 选择页占满可用高度：顶部工具条与底部统计不随滚动移动 */
.fill-height {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.fill-height .select-page {
  flex: 1;
  min-height: 0;
}

/* 输入页竖向占满窗口：两张卡分摊高度，卡片本身的空白也就被填满了 */
.parse-page:not(.fill-height) {
  display: flex;
  flex-direction: column;
  min-height: 100%;
}

/* 解析链接卡：高度按内容，**不参与拉伸**，多余高度全给结果卡。
   千万不要用 max-height 压它：上限小于内容最小高度时，被压的是盒子而不是内容，
   内容会溢出卡片、被下面的卡片盖住（实测踩过）。 */
.parse-page:not(.fill-height) > .card {
  display: flex;
  flex: 0 1 auto;
  flex-direction: column;
  min-height: 300px;
}

.parse-page:not(.fill-height) > .card textarea {
  flex: 1 1 auto;
  min-height: 110px;
  height: 110px;
}

/* 解析结果卡：高度**只由分配决定**（flex-basis: 0），不被明细列表的内容撑高。
   只有这样，卡片内的高度才是确定的，列表才能在里面滚动而不是把整页顶长。 */
.parse-page:not(.fill-height) > .results {
  /* 输入卡不拉伸，所以结果卡吃掉全部剩余高度 */
  flex: 1 1 0;
  /* 下限要装得下卡内固定部分 + 明细框的最小需要：
     卡片内边距 40 + 标题行 38 + 预设行 38 + 明细框（外边距 12 + 内边距 20 + 小标题 24
     + 列表下限 80）≈ 252，取 260。取值太小明细框会比卡片还高、列表被卡片边缘切掉 */
  min-height: 260px;
}

.parse-page:not(.fill-height) > .results .skipped-box {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  min-height: 0;
}

.parse-page:not(.fill-height) > .results .skipped-list {
  flex: 1 1 auto;
  /* 至少露出 3 行（每行 26px），不够就在框内滚 */
  min-height: 80px;
  max-height: none;
}

/* 选择内容页：解析结果独立成一页，表格 + 分批加载 */
.select-page {
  display: flex;
  flex-direction: column;
  padding: 0;
  overflow: hidden;
}

.select-bar .ghost,
.select-bar .primary {
  flex: none;
  padding: 6px 13px;
  font-size: 12.5px;
  border-radius: var(--radius-sm);
}

.select-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 10px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--line-soft);
}

/* 计数与动作成组：整组靠右（margin-left:auto），窄到放不下时整组换行，
   换行后组内也保持右对齐——标签继续留在左边 */
.bar-right {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  margin-left: auto;
}

/* 窄版"每批 N"：不带下拉箭头（原生 select 的箭头去不掉，所以自绘菜单） */
.batch-picker {
  position: relative;
  flex: none;
}

.ghost.compact {
  padding: 6px 10px;
  font-size: 12.5px;
}

.ghost.compact.on {
  color: var(--accent);
  border-color: var(--accent-line);
  background: var(--raised);
}

.batch-pop {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 15;
  min-width: 108px;
  padding: 4px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  box-shadow: 0 10px 28px rgba(20, 12, 16, 0.24);
}

.batch-item {
  display: block;
  width: 100%;
  padding: 6px 9px;
  font-size: 12.5px;
  color: var(--text);
  text-align: left;
  border-radius: var(--radius-sm);
}

.batch-item:hover {
  background: var(--hover);
}

.batch-item.on {
  color: var(--accent);
}

.back {
  display: grid;
  flex: none;
  place-items: center;
  width: 28px;
  height: 28px;
  color: var(--muted);
  border-radius: var(--radius-sm);
}

.back:hover {
  color: var(--text);
  background: var(--hover);
}

.back svg {
  width: 17px;
  height: 17px;
}

/* 标题按内容占宽（长标题折行，不会被截断），这样类型标签才能紧跟在标题后面 */
/* 一行布局下标题可伸缩：优先吃掉空白，实在放不下才省略（悬停看全名） */
.select-title {
  /* 不伸长：换行时若标题伸长，会把「合集」标签顶到整行最右 */
  flex: 0 1 auto;
  /* 超长标题也不能吃掉整行：给标签与右侧动作留出位置，超出部分省略号 */
  max-width: calc(100% - 460px);
  /* 下限保证标题至少能读；一行实在放不下时工具条整行换行兜底 */
  /* 只留一个很小的下限：短标题（如「消失的人」）不该撑出 110px 的盒子，
     否则标签会被推得离标题很远。长标题仍然按 max-width 省略。 */
  min-width: 28px;
  margin: 0;
  font-size: 14px;
  font-weight: 700;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.kind-tag {
  flex: none;
  /* 和标题靠拢：整行 gap 是 10px，这里收掉 4px */
  margin-left: -4px;
  padding: 2px 8px;
  font-size: 11px;
  color: var(--accent);
  background: var(--accent-soft);
  border: 1px solid var(--accent-line);
  border-radius: 999px;
}

.loaded-hint {
  font-size: 12px;
  color: var(--muted);
  white-space: nowrap;
}

.inline-field {
  display: inline-flex;
  flex: none;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--muted);
  white-space: nowrap;
}

.inline-field select {
  height: 28px;
  padding: 0 8px;
  font-size: 12.5px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.table-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.batch-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 12.5px;
}

.batch-table th {
  position: sticky;
  top: 0;
  z-index: 1;
  padding: 9px 10px;
  font-weight: 600;
  color: var(--muted);
  text-align: left;
  background: var(--card);
  border-bottom: 1px solid var(--line);
}

/* 分组行：多来源时用它把各组分开，一眼看出哪几行属于哪个来源 */
.batch-table tr.group-row {
  cursor: pointer;
  user-select: none;
}

/* 分组选择框：和行内的勾选框左对齐，别被折叠点击抢走事件 */
.group-check {
  margin-right: 8px;
  vertical-align: -3px;
}

.fold-arrow {
  width: 13px;
  height: 13px;
  margin-right: 4px;
  color: var(--faint);
  vertical-align: -2px;
  transform: rotate(90deg);
  transition: transform 0.15s ease;
}

.batch-table tr.group-row.folded .fold-arrow {
  transform: rotate(0deg);
}

.batch-table tr.group-row.folded .group-title {
  color: var(--muted);
}

.batch-table tr.group-row:hover .fold-arrow {
  color: var(--accent);
}

.batch-table tr.group-row td {
  padding: 7px 10px 6px;
  background: var(--raised);
  border-top: 1px solid var(--line);
  border-bottom: 1px solid var(--line-soft);
}

.batch-table tbody tr.group-row:first-child td {
  border-top: none;
}

.batch-table tr.group-row:hover td {
  background: var(--raised);
}

.group-kind {
  margin-right: 8px;
  padding: 1px 7px;
  font-size: 11px;
  color: var(--accent);
  background: var(--accent-soft);
  border: 1px solid var(--accent-line);
  border-radius: 999px;
}

.group-title {
  margin-right: 8px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text);
}

.batch-table td {
  padding: 8px 10px;
  border-bottom: 1px solid var(--line-soft);
}

.batch-table tbody tr:hover td {
  background: var(--raised);
}

.batch-table tr.on td {
  background: var(--accent-soft);
}

.col-check {
  width: 36px;
}

.col-idx {
  width: 58px;
  color: var(--faint);
}

/* 表头文字对齐要压过 .batch-table th 的 left */
.batch-table th.col-owner,
.batch-table td.col-owner {
  width: 150px;
  text-align: center;
}

.batch-table th.col-dur,
.batch-table td.col-dur {
  width: 84px;
  text-align: center;
}



.col-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.select-foot {
  display: flex;
  align-items: center;
  gap: 18px;
  padding: 11px 16px;
  border-top: 1px solid var(--line-soft);
}

.foot-count {
  font-size: 12.5px;
  color: var(--muted);
}

.foot-count b {
  color: var(--text);
  font-weight: 600;
}

/* 输入页里的批量来源条目：点它进选择页 */
.batch-entry {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
}

/* 解析结果 */
.results {
  margin-top: 16px;
}

.results-head {
  display: flex;
  align-items: center;
  gap: 9px;
  margin-bottom: 8px;
}

h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
}

.count {
  font-size: 12px;
  color: var(--faint);
}

.list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.item {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 12px 14px;
  background: var(--raised);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius);
}

.item.failed {
  background: var(--fail-bg);
  border-color: var(--fail-line);
}

.thumb {
  flex: none;
  display: grid;
  place-items: center;
  width: 92px;
  height: 58px;
  border-radius: var(--radius-sm);
  overflow: hidden;
  background: var(--thumb);
}

.thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.thumb-placeholder {
  width: 24px;
  height: 24px;
  color: var(--faint);
}

.meta {
  flex: 1;
  min-width: 0;
}

.title {
  margin: 0;
  font-size: 13.5px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sub {
  display: flex;
  gap: 12px;
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--muted);
}

.sub .faint {
  color: var(--faint);
}

.note {
  margin: 5px 0 0;
  font-size: 11.5px;
  color: var(--warn);
}

.error {
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--err);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: none;
}

.field span {
  font-size: 11px;
  color: var(--faint);
}

select {
  min-width: 158px;
  padding: 7px 9px;
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

select:focus {
  outline: none;
  border-color: var(--accent-line);
}

option:disabled {
  color: var(--faint);
}

.remove {
  flex: none;
  width: 26px;
  height: 26px;
  font-size: 12px;
  color: var(--faint);
  border-radius: var(--radius-sm);
}

.remove:hover {
  color: var(--err);
  background: var(--fail-bg);
}

.login-tip {
  margin: 14px 0 0;
  padding: 9px 12px;
  font-size: 12px;
  color: var(--accent-dark);
  background: var(--accent-soft);
  border-radius: var(--radius-sm);
}

/* 批量来源卡片 */
.batch {
  flex: 1;
  min-width: 0;
}

.batch-head {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.kind {
  flex: none;
  padding: 2px 9px;
  font-size: 11.5px;
  font-weight: 600;
  color: var(--accent-dark);
  background: var(--accent-soft);
  border-radius: 999px;
}

.batch-tools {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 10px 0 6px;
}

.mini {
  padding: 3px 10px;
  font-size: 11.5px;
  color: var(--muted);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.mini:hover {
  color: var(--text);
  border-color: var(--accent-line);
}

.batch-list {
  list-style: none;
  margin: 0;
  padding: 0;
  max-height: 320px;
  overflow-y: auto;
  border: 1px solid var(--line-soft);
  border-radius: var(--radius-sm);
  background: var(--card);
}

.batch-list li + li {
  border-top: 1px solid var(--line-soft);
}

.batch-list .check {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 7px 11px;
  font-size: 12.5px;
  cursor: pointer;
}

.batch-list .check:hover {
  background: var(--raised);
}

/* 勾选框外观统一在 styles.css 里定义，这里只管它不被压缩 */
.batch-list input {
  flex: none;
}

.idx {
  flex: none;
  width: 26px;
  color: var(--faint);
  font-size: 11.5px;
  text-align: right;
}

.entry-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.entry-dur {
  flex: none;
  color: var(--faint);
  font-size: 11.5px;
}

/* 「每批 N」上的下拉箭头，以及「继续解析 + 解析方式」按钮 */
.batch-picker .ghost.compact {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.caret {
  flex: none;
  width: 14px;
  height: 14px;
}

/* 边框画在容器上、两个按钮透明无边框：箭头因此看起来是在按钮里面 */
.parse-split {
  position: relative;
  display: inline-flex;
  align-items: stretch;
  flex: none;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.parse-split .seg {
  display: inline-flex;
  align-items: center;
  padding: 6px 4px 6px 11px;
  font-size: 12.5px;
  color: inherit;
  background: none;
  border: 0;
  border-radius: 0;
}

.parse-split .seg:first-of-type {
  border-top-left-radius: var(--radius-sm);
  border-bottom-left-radius: var(--radius-sm);
}

.parse-split .seg:hover:not(:disabled) {
  color: var(--accent);
}

.parse-split .seg:disabled {
  opacity: 0.55;
}

.parse-split .seg.arrow {
  padding: 6px 9px 6px 4px;
  border-top-right-radius: var(--radius-sm);
  border-bottom-right-radius: var(--radius-sm);
}

.parse-split .seg.arrow svg {
  display: block;
  width: 14px;
  height: 14px;
  color: var(--muted);
}

.parse-split .seg.arrow:hover:not(:disabled) svg {
  color: var(--accent);
}

.parse-split.on {
  border-color: var(--accent-line);
  background: var(--raised);
}

.parse-split.on .seg,
.parse-split.on .seg.arrow svg {
  color: var(--accent);
}

.parse-pop {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 15;
  min-width: 186px;
  padding: 4px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  box-shadow: 0 12px 30px rgba(20, 12, 16, 0.24);
}

.parse-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 9px;
  font-size: 12.5px;
  color: var(--text);
  text-align: left;
  white-space: nowrap;
  border-radius: var(--radius-sm);
}

.parse-item svg {
  flex: none;
  width: 15px;
  height: 15px;
  color: var(--muted);
}

.parse-item:hover {
  background: var(--hover);
}

.parse-item:hover svg {
  color: var(--text);
}

/* 「按序号加载」的小表单：菜单原地变成输入 */
.parse-range {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 9px 7px;
  font-size: 12.5px;
  color: var(--muted);
  white-space: nowrap;
}

.parse-range input {
  width: 76px;
  padding: 4px 7px;
  font-size: 12.5px;
  color: var(--text);
  text-align: right;
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.select-bar .parse-range .ghost.small {
  padding: 4px 10px;
  font-size: 12px;
}
</style>
