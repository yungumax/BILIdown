<script setup>
import Icon from "../components/Icon.vue";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import * as api from "../api";

const props = defineProps({
  login: { type: Object, required: true },
  /** 已保存的设置（后端结构） */
  settings: { type: Object, default: null },
  env: { type: Object, default: null },
});
const emit = defineEmits(["toast", "save", "reset", "reload", "login", "logout"]);

// 分类图标：值是 iconfont 图标名（见 src/icons.js）
const CATEGORY_ICONS = {
  download: "download",
  media: "album",
  naming: "fileText",
  folder: "folder",
  encode: "slidersH",
  update: "refresh",
  network: "server",
};

const categories = [
  { key: "download", label: "下载", hint: "目录、并发与恢复" },
  { key: "media", label: "媒体", hint: "清晰度、封装与封面字幕" },
  { key: "naming", label: "文件命名", hint: "模板与重名处理" },
  { key: "folder", label: "文件夹", hint: "层级与文件夹命名" },
  { key: "encode", label: "编码与处理", hint: "编码、分段与 FFmpeg" },
  { key: "update", label: "应用更新", hint: "版本检测与安装" },
  { key: "network", label: "网络与维护", hint: "代理、日志与数据" },
];

const active = ref("download");
const activeCategory = computed(() =>
  categories.find((category) => category.key === active.value)
);

/** 本地草稿：编辑期间不落盘，点「保存」才提交 */
const draft = ref(null);
// 命名预设只决定"条目自己叫什么"，**不写目录层级**：层级由「文件夹」页的规则负责。
// 所以这里一律不含 `/`，也不重复使用文件夹模板里的变量（{owner_name}/{collection_title}）。
// 每个预设对应**一种来源形状**，多了就是重复：
// 单条（{title}）／多P（分P）／批量列表（序号）／番剧课程（集）。
// 想在自己的文件里再挂点别的（比如 {quality}），存成"我的预设"即可。
// 「单文件（默认）」就是"没挑别的时用的那一个"：模板等于后端 naming_template 的兜底值，
// 软件没设置过命名模板时用它的命名。文件夹那边同理（见 FOLDER_PRESETS 的第一项）。
const BUILTIN_PRESETS = [
  { name: "单文件（默认）", template: "{title}.{ext}" },
  { name: "分P视频", template: "P{part_index} - {part_title}.{ext}" },
  { name: "合集/列表", template: "{index} {title}.{ext}" },
  { name: "番剧/课程", template: "第{episode_index}集 - {episode_title}.{ext}" },
];

/** 内置名不许被用户预设顶掉：更严的做法，重名直接拒绝保存。
 *  （旧做法是同名照存，结果下拉里两个同名项、点哪个都走内置，白存一个。） */
function builtinNameTaken(name, builtins) {
  return builtins.some((preset) => preset.name === name);
}

/** 预设名由模板反推：改了模板下拉就跟着变，不会停留在旧预设名上。
 *  自己的预设排在前面——刚存完要能在下拉里看见自己起的名字，
 *  哪怕模板和内置信一模一样（模板相同，叫什么由用户说了算）。 */
const selectedPreset = computed(() => {
  const template = draft.value?.naming_template ?? "";
  const saved = (draft.value?.naming_presets ?? []).find((preset) => preset.template === template);
  if (saved) return saved.name;
  const builtin = BUILTIN_PRESETS.find((preset) => preset.template === template);
  return builtin ? builtin.name : "";
});
const presetName = ref("");
const folderPresetName = ref("");

const proxyDraft = ref("");
const ffmpegDraft = ref("");
const dataDirDraft = ref("");
const updateResult = ref(null);
const checkingUpdate = ref(false);
const cleaning = ref("");

const dirty = computed(() => {
  if (!props.settings || !draft.value) return false;
  return JSON.stringify(draft.value) !== JSON.stringify(props.settings);
});

function beginDraft() {
  draft.value = props.settings ? { ...props.settings } : null;
}
beginDraft();

// 优先顺序列表里的画质项：带上档位号，和 B 站文档里的 qn 对得上
const QUALITY_PREFS = [
  { value: 127, label: "8K / 127" },
  { value: 126, label: "杜比视界 / 126" },
  { value: 125, label: "HDR / 125" },
  { value: 120, label: "4K / 120" },
  { value: 116, label: "1080P60 / 116" },
  { value: 112, label: "1080P+ / 112" },
  { value: 80, label: "1080P / 80" },
  { value: 74, label: "720P60 / 74" },
  { value: 64, label: "720P / 64" },
  { value: 32, label: "480P / 32" },
  { value: 16, label: "360P / 16" },
];

// 媒体页上面的「视频清晰度」下拉：0 = 最优画质（不高于任何档 → 取可用最高），
// 其余选项复用优先顺序表里的档位（带 qn，好对文档）
const QUALITY_CHOICES = [{ value: 0, label: "最优画质" }, ...QUALITY_PREFS];

const AUDIO_PREFS = [
  { value: "auto", label: "最佳可用" },
  { value: "flac", label: "Hi-Res 无损" },
  { value: "dolby", label: "杜比全景声" },
  { value: "normal", label: "普通音轨" },
];

// 视频格式（封装）：MKV 已去掉——封面、字幕、弹幕都成独立文件后它没用了
const VIDEO_FORMATS = [
  { value: "mp4", label: "MP4（通用）" },
  { value: "ts", label: "TS（剪辑 / 直播工具友好）" },
];

// 音频格式：只影响「音频」来源的成品，视频里的音轨保持原编码
const AUDIO_FORMATS = [
  { value: "source", label: "原格式（不转码）" },
  { value: "mp3", label: "MP3（兼容最广）" },
];

// 图片格式：只影响图文图片与封面
const IMAGE_FORMATS = [
  { value: "source", label: "原格式（不转码）" },
  { value: "jpg", label: "JPG（体积小）" },
];


const CODECS = [
  { value: "auto", label: "不限编码" },
  { value: "avc", label: "优先 AVC" },
  { value: "hevc", label: "优先 HEVC" },
  { value: "av1", label: "优先 AV1" },
];

const FALLBACKS = [
  { value: "nearest", label: "回退到最佳可用" },
  { value: "fail", label: "直接失败（不下载）" },
];

const RANGES = [
  { value: false, label: "仅下载最终视频（最快）" },
  { value: true, label: "同时保留原始视频/音频频道" },
];

const RENAME_CONFLICTS = [
  { value: "skip", label: "已有文件则跳过（推荐）" },
  { value: "overwrite", label: "覆盖已有文件" },
  { value: "auto", label: "自动重命名（追加序号）" },
];

const LOG_LEVELS = [
  { value: "debug", label: "调试" },
  { value: "info", label: "信息" },
  { value: "warn", label: "警告" },
  { value: "error", label: "错误" },
];

const PARSE_PRESETS = [
  { value: "标准", batch: 8, wait: 1000, every: 100, rest: 3000 },
  { value: "快速", batch: 16, wait: 500, every: 200, rest: 2000 },
  { value: "谨慎", batch: 3, wait: 2000, every: 50, rest: 5000 },
];

const CONCURRENCY = [1, 2, 3, 4, 5];
const RETRIES = [0, 1, 2, 3, 5, 8];
const SPEEDS = [0, 1, 2, 5, 10, 20, 50];
const SEGMENTS = [1, 2, 4, 6, 8, 12, 16];

const proxyValue = computed({
  get: () => (proxyDraft.value !== "" ? proxyDraft.value : draft.value?.proxy ?? ""),
  set: (value) => (proxyDraft.value = value),
});
const ffmpegValue = computed({
  get: () => (ffmpegDraft.value !== "" ? ffmpegDraft.value : draft.value?.ffmpeg_path ?? ""),
  set: (value) => (ffmpegDraft.value = value),
});
const dataDirValue = computed({
  get: () => (dataDirDraft.value !== "" ? dataDirDraft.value : draft.value?.data_dir ?? ""),
  set: (value) => (dataDirDraft.value = value),
});

/** 本地时区的 YYYY-MM-DD；不带参数即今天 */
function localDate(unixSecs) {
  const d = unixSecs === undefined ? new Date() : new Date(unixSecs * 1000);
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

// 预览走后端同一个渲染器：预览里能出什么，落盘就能出什么
const namingPreview = ref("");
// 文件夹层级：输入框引用 + 预览 + 弹出面板状态
const folderInput = ref(null);
const folderPreview = ref("");
const pickingFolderVar = ref(false);
const folderPicker = ref(null);

/** 两套模板的变量重合提醒：同一个变量在文件名与文件夹里都出现 = 信息重复 */
const overlapVars = computed(() => {
  const pick = (tpl) => [...String(tpl ?? "").matchAll(/\{(\w+)\}/g)].map((m) => m[1]);
  const folder = new Set(pick(draft.value?.folder_template));
  const duplicated = [...new Set(pick(draft.value?.naming_template))]
    .filter((v) => folder.has(v))
    .map((v) => `{${v}}`);
  return { folderOnly: [...folder], duplicated };
});

const FOLDER_PRESETS = [
  // 第一条 = 后端的 DEFAULT_FOLDER_TEMPLATE：软件没设置过文件夹模板时用它
  { name: "UP → 合集 → 条目（默认）", template: "{owner_name}/{collection_title}" },
  { name: "UP → 来源类型（图文/音频不混在一起）", template: "{owner_name}/{source_kind}" },
  { name: "只按 UP 分层", template: "{owner_name}" },
  { name: "不建文件夹（全部平铺）", template: "" },
];
let previewSeq = 0;

async function refreshPreview() {
  const template = draft.value?.naming_template ?? "";
  const seq = ++previewSeq;
  try {
    const text = await api.previewNaming(template, {
      date: localDate(),
      publish_date: "2026-01-02",
      ext: draft.value?.container === "mkv" ? "mkv" : "mp4",
    });
    // 连续敲键会有多次请求，只认最后一次的结果
    if (seq === previewSeq) namingPreview.value = text;
  } catch {
    if (seq === previewSeq) namingPreview.value = "";
  }

  const folder = draft.value?.folder_template ?? "";
  const fseq = ++previewSeq;
  try {
    const text = await api.previewNaming(folder, { date: localDate(), dir: true });
    if (fseq === previewSeq) folderPreview.value = text;
  } catch {
    if (fseq === previewSeq) folderPreview.value = "";
  }
}

watch(
  () => [draft.value?.naming_template, draft.value?.folder_template, draft.value?.container],
  refreshPreview,
  { immediate: true }
);

// 变量清单与插入面板
const variables = ref([]);
const pickingVar = ref(false);
const varPicker = ref(null);
const templateInput = ref(null);

/** 面板里的栏目：一列一组，横排。
 *  顺序就是 Rust 那张表的顺序，同名栏目保证连续（naming.rs 的
 *  variable_sections_stay_contiguous 守着），所以遇到新名字就新开一列即可。 */
const variableColumns = computed(() => {
  const out = [];
  for (const item of variables.value) {
    const name = item.section || "其他";
    if (out[out.length - 1]?.name !== name) out.push({ name, items: [] });
    out[out.length - 1].items.push(item);
  }
  return out;
});

async function loadVariables() {
  try {
    const list = await api.namingVariables();
    // 显示文本在这里拼好：模板里直接写 `{...}` 会和 Vue 的插值定界符打架
    variables.value = list.map((item) => ({ ...item, text: `{${item.token}}` }));
  } catch (error) {
    // 不静默吞：面板空掉时必须能在控制台看到真实原因
    console.error("魔法变量加载失败:", error);
    variables.value = [];
  }
}

/** 插到光标处；没有焦点时追加到末尾，插完把光标放到标记之后。
 *  target 决定写进哪个字段：文件名模板 or 文件夹层级模板。 */
function insertToken(token, target = "naming") {
  if (!draft.value) return;
  const snippet = `{${token}}`;
  const key = target === "folder" ? "folder_template" : "naming_template";
  const el = target === "folder" ? folderInput.value : templateInput.value;
  if (!el) {
    draft.value[key] = `${draft.value[key] ?? ""}${snippet}`;
    return;
  }
  const start = el.selectionStart ?? el.value.length;
  const end = el.selectionEnd ?? start;
  draft.value[key] = el.value.slice(0, start) + snippet + el.value.slice(end);
  nextTick(() => {
    el.focus();
    const caret = start + snippet.length;
    el.setSelectionRange(caret, caret);
  });
}

function onVarDocumentDown(event) {
  if (pickingVar.value && varPicker.value && !varPicker.value.contains(event.target)) {
    pickingVar.value = false;
  }
  if (pickingFolderVar.value && folderPicker.value && !folderPicker.value.contains(event.target)) {
    pickingFolderVar.value = false;
  }
}

function onVarKeydown(event) {
  if (event.key === "Escape") {
    pickingVar.value = false;
    pickingFolderVar.value = false;
  }
}

onMounted(() => {
  loadVariables();
  document.addEventListener("mousedown", onVarDocumentDown);
  document.addEventListener("keydown", onVarKeydown);
});

onUnmounted(() => {
  document.removeEventListener("mousedown", onVarDocumentDown);
  document.removeEventListener("keydown", onVarKeydown);
});

const parsePaceNote = computed(() => {
  const batch = draft.value?.parse_batch ?? 8;
  const wait = draft.value?.parse_batch_wait_ms ?? 1000;
  const every = draft.value?.parse_rest_every ?? 100;
  const rest = draft.value?.parse_rest_ms ?? 3000;
  const cap = draft.value?.parse_cap ?? 0;
  const total = cap > 0 ? cap : 200;
  const batches = Math.ceil(total / batch);
  const extraMs = Math.max(batches - 1, 0) * wait + Math.floor(total / every) * rest;
  const capNote = cap > 0 ? `单次上限 ${cap} 条，` : "";
  return `${capNote}解析约 ${total} 条，额外等待约 ${Math.round(extraMs / 1000)} 秒，不含网络耗时。`;
});

function set(key, value) {
  if (draft.value) draft.value[key] = value;
}

/** 优先顺序列表：添加 / 上移 / 下移 / 删除 */
function addQualityPref() {
  if (!draft.value) return;
  const prefs = draft.value.quality_prefs ?? [];
  const last = prefs[prefs.length - 1];
  draft.value.quality_prefs = [...prefs, { qn: last?.qn ?? 127, codec: "auto" }];
}

function addAudioPref() {
  if (!draft.value) return;
  const prefs = draft.value.audio_prefs ?? [];
  draft.value.audio_prefs = [...prefs, "auto"];
}

function movePref(key, index, delta) {
  if (!draft.value) return;
  const list = [...(draft.value[key] ?? [])];
  const target = index + delta;
  if (target < 0 || target >= list.length) return;
  [list[index], list[target]] = [list[target], list[index]];
  draft.value[key] = list;
}

function removePref(key, index) {
  if (!draft.value) return;
  const list = [...(draft.value[key] ?? [])];
  // 可以删到一条不剩：空表 = 没自定义，挑流回到上面两个单值（后端也是这个约定）
  list.splice(index, 1);
  draft.value[key] = list;
}

/** 选预设：内置或用户保存的，选中即填入模板（下拉显示什么由模板决定） */
function selectPreset(name) {
  if (!name || !draft.value) return;
  const builtin = BUILTIN_PRESETS.find((preset) => preset.name === name);
  if (builtin) {
    draft.value.naming_template = builtin.template;
    return;
  }
  const saved = draft.value.naming_presets?.find((preset) => preset.name === name);
  if (saved) draft.value.naming_template = saved.template;
}

/** 文件夹页的预设名同样由模板反推，「自定义模板」就是没有预设对得上的时候。
 *  自己的预设优先，理由同命名预设。 */
const selectedFolderPreset = computed(() => {
  const template = draft.value?.folder_template ?? "";
  const saved = (draft.value?.folder_presets ?? []).find((preset) => preset.template === template);
  if (saved) return saved.name;
  const builtin = FOLDER_PRESETS.find((preset) => preset.template === template);
  return builtin ? builtin.name : "";
});

function selectFolderPreset(name) {
  if (!name || !draft.value) return;
  const builtin = FOLDER_PRESETS.find((preset) => preset.name === name);
  if (builtin) {
    draft.value.folder_template = builtin.template;
    return;
  }
  const saved = draft.value.folder_presets?.find((preset) => preset.name === name);
  if (saved) draft.value.folder_template = saved.template;
}

/** 保存文件夹预设：和命名预设同一套做法，只是模板可以是空的（不建文件夹） */
function saveFolderPreset() {
  const name = folderPresetName.value.trim();
  if (!name || !draft.value) {
    emit("toast", "先填写预设名称，再保存为预设");
    return;
  }
  if (builtinNameTaken(name, FOLDER_PRESETS)) {
    emit("toast", `「${name}」和内置层级重名了，换一个名字（内置清单不会被覆盖）`);
    return;
  }
  const presets = [...(draft.value.folder_presets ?? [])];
  const existing = presets.findIndex((preset) => preset.name === name);
  if (existing >= 0) {
    presets[existing] = { name, template: draft.value.folder_template };
  } else {
    presets.push({ name, template: draft.value.folder_template });
  }
  draft.value.folder_presets = presets;
  folderPresetName.value = "";
  emit("toast", `预设「${name}」已加入，点上方「保存」生效`);
}

/** 保存为预设：同名更新模板，随设置一起落盘 */
function savePreset() {
  const name = presetName.value.trim();
  if (!name || !draft.value) {
    emit("toast", "先填写预设名称，再保存为预设");
    return;
  }
  if (builtinNameTaken(name, BUILTIN_PRESETS)) {
    emit("toast", `「${name}」和内置预设重名了，换一个名字（内置清单不会被覆盖）`);
    return;
  }
  const presets = [...(draft.value.naming_presets ?? [])];
  const existing = presets.findIndex((preset) => preset.name === name);
  if (existing >= 0) {
    presets[existing] = { name, template: draft.value.naming_template };
  } else {
    presets.push({ name, template: draft.value.naming_template });
  }
  draft.value.naming_presets = presets;
  // 不用手动设下拉：下拉是从模板反推的，这里替换进清单后它自己就会显示这个名字
  presetName.value = "";
  emit("toast", `预设「${name}」已加入，点上方「保存」生效`);
}

function applyPreset(name) {
  const preset = PARSE_PRESETS.find((item) => item.value === name);
  if (!preset || !draft.value) return;
  draft.value.parse_preset = preset.value;
  draft.value.parse_batch = preset.batch;
  draft.value.parse_batch_wait_ms = preset.wait;
  draft.value.parse_rest_every = preset.every;
  draft.value.parse_rest_ms = preset.rest;
}

async function chooseDir() {
  const dir = await api.chooseOutputDir().catch(() => "");
  if (dir) set("output_dir", dir);
}

async function chooseFfmpeg() {
  const file = await api.pickFfmpeg().catch(() => "");
  if (file) {
    ffmpegDraft.value = "";
    set("ffmpeg_path", file);
  }
}

async function chooseDataDir() {
  const dir = await api.chooseOutputDir().catch(() => "");
  if (dir) {
    dataDirDraft.value = "";
    set("data_dir", dir);
  }
}

function commitProxy() {
  set("proxy", proxyValue.value.trim());
  proxyDraft.value = "";
}

function save() {
  if (!dirty.value) return;
  emit("save", { ...draft.value });
}

function undo() {
  beginDraft();
  proxyDraft.value = "";
  ffmpegDraft.value = "";
  dataDirDraft.value = "";
  emit("reload");
}

function reset() {
  emit("reset");
  beginDraft();
}

async function checkUpdate() {
  checkingUpdate.value = true;
  try {
    updateResult.value = await api.checkUpdates();
  } catch (error) {
    updateResult.value = { current: props.settings?.version || "", latest: "", up_to_date: false, error: String(error) };
  } finally {
    checkingUpdate.value = false;
  }
}

async function cleanup(kind) {
  cleaning.value = kind;
  try {
    if (kind === "temp") {
      const count = await api.cleanupTemp();
      emit("toast", count ? `已清理 ${count} 项临时文件` : "没有可清理的临时文件");
    } else if (kind === "cache") {
      await api.cleanupCache();
      emit("toast", "已清理网络缓存（会话已重建）");
    } else if (kind === "diag") {
      const path = await api.exportDiagnostics();
      emit("toast", path ? `诊断信息已导出：${path}` : "已取消导出");
    }
  } catch (error) {
    emit("toast", String(error));
  } finally {
    cleaning.value = "";
  }
}

async function open(path) {
  if (!path) return;
  try {
    await api.openPath(path);
  } catch (error) {
    emit("toast", String(error));
  }
}
</script>

<template>
  <div>
    <section class="card">
      <header class="head">
        <h1>设置</h1>
        <span class="spacer"></span>
        <button class="ghost" :disabled="!settings" @click="reset">
          <Icon name="reload" />
          恢复默认
        </button>
        <button class="ghost" :disabled="!dirty" @click="undo">
          <Icon name="undo" />
          撤销
        </button>
        <button class="primary" :disabled="!dirty" @click="save">
          <Icon name="check" />
          保存
        </button>
      </header>

      <div v-if="draft" class="layout">
        <nav class="cats">
          <button
            v-for="category in categories"
            :key="category.key"
            :class="{ active: active === category.key }"
            @click="active = category.key"
          >
            <Icon :name="CATEGORY_ICONS[category.key]" class="icon" />
            <span class="text">
              <span class="label">{{ category.label }}</span>
              <span class="hint">{{ category.hint }}</span>
            </span>
          </button>
        </nav>

        <div class="panel">
          <div class="panel-head">
                          <Icon :name="CATEGORY_ICONS[activeCategory.key]" class="panel-icon" />
            <div>
              <h2>{{ activeCategory.label }}</h2>
              <p class="panel-hint">{{ activeCategory.hint }}</p>
            </div>
          </div>

          <!-- 下载 -->
          <div v-if="active === 'download'" class="fields">
            <div class="field full">
              <label>保存目录</label>
              <div class="row-flex">
                <input v-model="draft.output_dir" spellcheck="false" />
                <button class="ghost" @click="chooseDir">选择</button>
              </div>
            </div>

            <div class="grid2">
              <div class="field">
                <label>同时下载任务数</label>
                <select v-model.number="draft.max_concurrent_tasks">
                  <option v-for="n in CONCURRENCY" :key="n" :value="n">{{ n }}</option>
                </select>
              </div>
              <div class="field">
                <label>失败自动重试次数</label>
                <select v-model.number="draft.retry_count">
                  <option v-for="n in RETRIES" :key="n" :value="n">{{ n }}</option>
                </select>
              </div>
            </div>

            <div class="field full">
              <label>全局下载限速（MiB/s）</label>
              <select v-model.number="draft.speed_limit_mib">
                <option v-for="n in SPEEDS" :key="n" :value="n">
                  {{ n === 0 ? "不限速" : `${n} MiB/s` }}
                </option>
              </select>
            </div>

            <div class="checks full">
              <label class="check">
                <input type="checkbox" v-model="draft.auto_refresh_urls" />
                <span>链接过期时自动刷新</span>
              </label>
              <label class="check">
                <input type="checkbox" v-model="draft.resume_on_start" />
                <span>启动时自动继续未完成任务</span>
              </label>
            </div>

            <div class="group full">
              <div class="group-title">
                解析节奏
                <span class="info" title="批量解析时按此节奏分批请求，降低触发风控的概率">?</span>
              </div>
              <div class="grid2">
                <div class="field">
                  <label>解析预设</label>
                  <select
                    :value="draft.parse_preset"
                    @change="applyPreset($event.target.value)"
                  >
                    <option v-for="preset in PARSE_PRESETS" :key="preset.value" :value="preset.value">
                      {{ preset.value }}
                    </option>
                  </select>
                </div>
                <div class="field">
                  <label>每批解析 <span class="info" title="一轮里同时解析几条">?</span></label>
                  <select v-model.number="draft.parse_batch">
                    <option :value="3">3 条</option>
                    <option :value="8">8 条</option>
                    <option :value="16">16 条</option>
                    <option :value="30">30 条</option>
                  </select>
                </div>
                <div class="field">
                  <label>批间等待 <span class="info" title="两轮解析之间等多久，给接口留出间隔">?</span></label>
                  <select v-model.number="draft.parse_batch_wait_ms">
                    <option :value="500">0.5 秒</option>
                    <option :value="1000">1 秒</option>
                    <option :value="2000">2 秒</option>
                    <option :value="3000">3 秒</option>
                  </select>
                </div>
                <div class="field">
                  <label>每 {{ draft.parse_rest_every }} 条休息 <span class="info" title="累计解析这么多条后额外休息一次">?</span></label>
                  <div class="row-flex">
                    <select v-model.number="draft.parse_rest_every">
                      <option :value="50">每 50 条</option>
                      <option :value="100">每 100 条</option>
                      <option :value="200">每 200 条</option>
                    </select>
                    <select v-model.number="draft.parse_rest_ms">
                      <option :value="2000">2 秒</option>
                      <option :value="3000">3 秒</option>
                      <option :value="5000">5 秒</option>
                    </select>
                  </div>
                </div>
                <div class="field">
                  <label>
                    单次解析上限
                    <span
                      class="info"
                      title="一次最多解析多少条。超过上限的来源（例如 1337 条投稿）用选择内容页「解析」里的「按序号加载」分几次拉完，两批互不重叠。0 表示按来源类型给默认值：合集/收藏夹 500、UP 空间 300"
                      >?</span
                    >
                  </label>
                  <select v-model.number="draft.parse_cap">
                    <option :value="0">默认（合集/收藏夹 500、UP 空间 300）</option>
                    <option :value="300">300 条</option>
                    <option :value="500">500 条</option>
                    <option :value="1000">1000 条</option>
                    <option :value="2000">2000 条</option>
                    <option :value="20000">不限制（最多 20000 条）</option>
                  </select>
                </div>
              </div>
              <p class="note">{{ parsePaceNote }}</p>
            </div>

            <div class="env full">
              <div class="env-head">
                <span class="env-title">运行环境 <b>FFmpeg</b></span>
                <span class="spacer"></span>
                <span class="badge" :class="env?.ffmpeg_ok ? 'ok' : 'bad'">
                  {{ env?.ffmpeg_ok ? "就绪" : "未找到" }}
                </span>
                <button class="ghost" @click="emit('reload')">重新检查</button>
              </div>
              <div class="env-row">
                <span class="env-name">FFmpeg</span>
                <span class="env-info" :title="env?.ffmpeg_info">{{ env?.ffmpeg_info || "未检测" }}</span>
                <span class="badge small" :class="env?.ffmpeg_ok ? 'ok' : 'bad'">
                  {{ env?.ffmpeg_ok ? "可用" : "不可用" }}
                </span>
              </div>
            </div>
          </div>

          <!-- 媒体 -->
          <div v-else-if="active === 'media'" class="fields">
            <div class="grid2">
              <div class="field">
                <label>视频清晰度</label>
                <select v-model.number="draft.default_quality">
                  <option v-for="item in QUALITY_CHOICES" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>音频质量</label>
                <select v-model="draft.default_audio">
                  <option v-for="item in AUDIO_PREFS" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </div>
            </div>

            <div class="grid3">
              <div class="field">
                <label>视频格式</label>
                <select v-model="draft.container">
                  <option v-for="item in VIDEO_FORMATS" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>音频格式</label>
                <select v-model="draft.audio_format">
                  <option v-for="item in AUDIO_FORMATS" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>图片格式</label>
                <select v-model="draft.image_format">
                  <option v-for="item in IMAGE_FORMATS" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </div>
            </div>
            <p class="note">
              视频格式只管封装（MP4 通用、TS 给剪辑工具）；音频格式只管「音频」来源的成品，
              视频里的音轨保持原编码；图片格式只管图文图片与封面。后两项选转码需要 ffmpeg。
            </p>

            <div class="grid2 full">
              <label class="check card-check">
                <input type="checkbox" v-model="draft.download_cover" />
                <span>下载封面（独立图片）</span>
              </label>
              <label class="check card-check">
                <input type="checkbox" v-model="draft.download_danmaku" />
                <span>下载弹幕（独立 .xml）</span>
              </label>
            </div>
            <p v-if="draft.download_cover || draft.download_danmaku" class="note">
              封面和弹幕都存成<b>与视频同名的独立文件</b>（封面按上面的图片格式、弹幕 .xml），
              不与视频合成，播放器直接读同名文件即可。音频与图文没有弹幕，会跳过。
            </p>

            <div class="field full">
              <label>下载范围</label>
              <select
                :value="draft.keep_temp ? 'true' : 'false'"
                @change="set('keep_temp', $event.target.value === 'true')"
              >
                <option v-for="item in RANGES" :key="item.label" :value="String(item.value)">
                  {{ item.label }}
                </option>
              </select>
            </div>

            <div class="sub-card full">
              <div class="sub-head">
                <Icon name="funnel" class="sub-icon" />
                <span class="sub-text">
                  <span class="sub-title">画质优先顺序</span>
                  <span class="sub-note">从上到下匹配画质与编码。</span>
                </span>
                <span class="spacer"></span>
                <button class="ghost" @click="addQualityPref">
                  <Icon name="plus" />
                  添加画质
                </button>
              </div>

              <div
                v-for="(pref, index) in draft.quality_prefs"
                :key="`q-${index}`"
                class="pref-row"
              >
                <span class="pref-no num">{{ index + 1 }}</span>
                <div class="field">
                  <label>第 {{ index + 1 }} 优先画质</label>
                  <select v-model.number="pref.qn">
                    <option v-for="item in QUALITY_PREFS" :key="item.value" :value="item.value">
                      {{ item.label }}
                    </option>
                  </select>
                </div>
                <div class="field">
                  <label>第 {{ index + 1 }} 优先编码</label>
                  <select v-model="pref.codec">
                    <option v-for="item in CODECS" :key="item.value" :value="item.value">
                      {{ item.label }}
                    </option>
                  </select>
                </div>
                <div class="pref-acts">
                  <button
                    class="act"
                    title="上移"
                    :disabled="index === 0"
                    @click="movePref('quality_prefs', index, -1)"
                  >
                    ↑
                  </button>
                  <button
                    class="act"
                    title="下移"
                    :disabled="index === draft.quality_prefs.length - 1"
                    @click="movePref('quality_prefs', index, 1)"
                  >
                    ↓
                  </button>
                  <button
                    class="act"
                    title="删除"
                    @click="removePref('quality_prefs', index)"
                  >
                    ×
                  </button>
                </div>
              </div>
              <p v-if="!draft.quality_prefs.length" class="note">
                尚未自定义，使用上方的视频清晰度和编码设置。
              </p>
              <p v-else class="note">逐条尝试，命中即用；编码偏好在同档位内生效，不会为编码牺牲清晰度。</p>
            </div>

            <div class="sub-card full">
              <div class="sub-head">
                <Icon name="microphone" class="sub-icon" />
                <span class="sub-text">
                  <span class="sub-title">音频优先顺序</span>
                  <span class="sub-note">独立选择音轨，再与选中的视频合并。</span>
                </span>
                <span class="spacer"></span>
                <button class="ghost" @click="addAudioPref">
                  <Icon name="plus" />
                  添加音质
                </button>
              </div>

              <div v-for="(kind, index) in draft.audio_prefs" :key="`a-${index}`" class="pref-row">
                <span class="pref-no num">{{ index + 1 }}</span>
                <div class="field">
                  <label>第 {{ index + 1 }} 优先音质</label>
                  <select v-model="draft.audio_prefs[index]">
                    <option v-for="item in AUDIO_PREFS" :key="item.value" :value="item.value">
                      {{ item.label }}
                    </option>
                  </select>
                </div>
                <div class="pref-acts">
                  <button
                    class="act"
                    title="上移"
                    :disabled="index === 0"
                    @click="movePref('audio_prefs', index, -1)"
                  >
                    ↑
                  </button>
                  <button
                    class="act"
                    title="下移"
                    :disabled="index === draft.audio_prefs.length - 1"
                    @click="movePref('audio_prefs', index, 1)"
                  >
                    ↓
                  </button>
                  <button
                    class="act"
                    title="删除"
                    @click="removePref('audio_prefs', index)"
                  >
                    ×
                  </button>
                </div>
              </div>
              <p v-if="!draft.audio_prefs.length" class="note">尚未自定义，使用上方的音频质量设置。</p>
              <p v-else class="note">链上都拿不到时退回普通音轨，不会出现没有音轨的任务。</p>
            </div>

            <div class="field full">
              <label>所有偏好都不可用时</label>
              <select v-model="draft.quality_fallback">
                <option v-for="item in FALLBACKS" :key="item.value" :value="item.value">
                  {{ item.label }}
                </option>
              </select>
            </div>
          </div>

          <!-- 文件命名 -->
          <div v-else-if="active === 'naming'" class="fields">
            <div class="field full">
              <label>命名预设</label>
              <select :value="selectedPreset" @change="selectPreset($event.target.value)">
                <option value="">自定义模板</option>
                <optgroup label="内置">
                  <option v-for="preset in BUILTIN_PRESETS" :key="preset.name" :value="preset.name">
                    {{ preset.name }}
                  </option>
                </optgroup>
                <optgroup v-if="draft.naming_presets?.length" label="我的预设">
                  <option
                    v-for="preset in draft.naming_presets"
                    :key="preset.name"
                    :value="preset.name"
                  >
                    {{ preset.name }}
                  </option>
                </optgroup>
              </select>
            </div>

            <div class="field full">
              <label>命名模板</label>
              <div class="row-flex">
                <input ref="templateInput" v-model="draft.naming_template" spellcheck="false" />
                <div ref="varPicker" class="var-picker">
                  <button
                    class="ghost"
                    :class="{ on: pickingVar }"
                    title="插入变量"
                    aria-haspopup="menu"
                    :aria-expanded="pickingVar"
                    @click="pickingVar = !pickingVar"
                  >
                    <Icon name="plus" />
                  </button>

                  <Transition name="picker">
                    <div v-if="pickingVar" class="var-panel">
                      <div class="var-head">
                        <b>魔法变量</b>
                        <span>点击后插入到光标位置</span>
                      </div>
                      <div class="var-grid">
                        <section v-for="column in variableColumns" :key="column.name" class="var-col">
                          <p class="var-col-name">{{ column.name }}</p>
                          <button
                            v-for="item in column.items"
                            :key="item.token"
                            class="var-item"
                            :title="item.hint || item.label"
                            @click="insertToken(item.token)"
                          >
                            <code>{{ item.text }}</code>
                            <span>{{ item.label }}</span>
                          </button>
                        </section>
                      </div>
                    </div>
                  </Transition>
                </div>
              </div>
              <p class="note">
                <b>命名规则决定"条目自己叫什么"</b>：视频与音频是文件名，图文与专栏是条目文件夹名。
                目录层级由「文件夹」页的规则决定，最终路径 = 文件夹规则 + 命名规则。
              </p>
              <p class="note">
                文件名预览：<b>{{ namingPreview }}</b>
              </p>
              <p v-if="overlapVars.duplicated.length" class="note warn">
                这些变量在「文件夹」模板里也用了：{{ overlapVars.duplicated.join("、") }}
                —— 路径里会出现重复信息（例如 UP 名既在目录又在文件名）。想让层级只由文件夹模板负责，就从文件名里去掉它们。
              </p>
            </div>

            <div class="field full">
              <label>预设名称</label>
              <div class="row-flex">
                <input
                  v-model="presetName"
                  spellcheck="false"
                  placeholder="例如：收藏用命名"
                  @keydown.enter="savePreset"
                />
                <button class="ghost" @click="savePreset">
                  <Icon name="check" />
                  保存为预设
                </button>
              </div>
              <p class="note">同名预设会更新模板。预设随设置一起保存（点上方「保存」生效），下次可直接选用。</p>
            </div>

            <div class="field full">
              <label>重名处理</label>
              <select v-model="draft.rename_conflict">
                <option v-for="item in RENAME_CONFLICTS" :key="item.value" :value="item.value">
                  {{ item.label }}
                </option>
              </select>
              <p class="note">
                {{
                  draft.rename_conflict === "skip"
                    ? "最终文件已存在时跳过整个任务，未完成的分片缓存仍会继续恢复。"
                    : draft.rename_conflict === "auto"
                      ? "文件名后追加 (1) (2) … 序号，直到不冲突。"
                      : "直接覆盖已存在的同名文件。"
                }}
              </p>
            </div>
          </div>

          <!-- 文件夹层级 -->
          <div v-else-if="active === 'folder'" class="fields">
            <div class="field full">
              <label>层级预设</label>
              <select :value="selectedFolderPreset" @change="selectFolderPreset($event.target.value)">
                <option value="">自定义模板</option>
                <optgroup label="内置">
                  <option v-for="preset in FOLDER_PRESETS" :key="preset.name" :value="preset.name">
                    {{ preset.name }}
                  </option>
                </optgroup>
                <optgroup v-if="draft.folder_presets?.length" label="我的预设">
                  <option
                    v-for="preset in draft.folder_presets"
                    :key="preset.name"
                    :value="preset.name"
                  >
                    {{ preset.name }}
                  </option>
                </optgroup>
              </select>
            </div>

            <div class="field full">
              <label>文件夹模板</label>
              <div class="row-flex">
                <input ref="folderInput" v-model="draft.folder_template" spellcheck="false" placeholder="留空表示不建文件夹" />
                <div ref="folderPicker" class="var-picker">
                  <button
                    class="ghost"
                    :class="{ on: pickingFolderVar }"
                    title="插入变量"
                    aria-haspopup="menu"
                    :aria-expanded="pickingFolderVar"
                    @click="pickingFolderVar = !pickingFolderVar"
                  >
                    <Icon name="plus" />
                  </button>

                  <Transition name="picker">
                    <div v-if="pickingFolderVar" class="var-panel">
                      <div class="var-head">
                        <b>魔法变量</b>
                        <span>点击后插入到光标位置</span>
                      </div>
                      <div class="var-grid">
                        <section v-for="column in variableColumns" :key="column.name" class="var-col">
                          <p class="var-col-name">{{ column.name }}</p>
                          <button
                            v-for="item in column.items"
                            :key="item.token"
                            class="var-item"
                            :title="item.hint || item.label"
                            @click="insertToken(item.token, 'folder')"
                          >
                            <code>{{ item.text }}</code>
                            <span>{{ item.label }}</span>
                          </button>
                        </section>
                      </div>
                    </div>
                  </Transition>
                </div>
              </div>
              <p class="note">
                文件夹预览：<b>{{ folderPreview || "（不建文件夹）" }}</b>
              </p>
            </div>

            <div class="field full">
              <label>预设名称</label>
              <div class="row-flex">
                <input
                  v-model="folderPresetName"
                  spellcheck="false"
                  placeholder="例如：按来源分目录"
                  @keydown.enter="saveFolderPreset"
                />
                <button class="ghost" @click="saveFolderPreset">
                  <Icon name="check" />
                  保存为预设
                </button>
              </div>
              <p class="note">
                同名预设会更新模板。预设随设置一起保存（点上方「保存」生效），下次可直接选用；
                模板留空也能存——"不建文件夹"就是个正当的预设。
              </p>
            </div>

            <div class="field full">
              <label>层级规则</label>
              <p class="note">
                解析 UP 链接（投稿 / 合集 / 收藏夹 / 系列 / 图文 / 音频）时按
                <b>UP → 合集 → 条目</b> 分层：合集名取自来源，没有合集（例如 UP 投稿、图文、音频）时那一层自动消失。
                单个视频或单条图文/专栏直链不会进 UP 目录，除非模板里写了变量。
                想区分同一 UP 的不同内容，可在模板里用 <code>{source_kind}</code>（合集 / 收藏夹 / 图文 / 音频 …）。
                条目自身的文件名仍由「文件命名」决定，最终路径 = 文件夹层级 + 文件名。
              </p>
            </div>
          </div>

          <!-- 编码与处理 -->
          <div v-else-if="active === 'encode'" class="fields">
            <div class="field full">
              <label>单任务分段数</label>
              <select v-model.number="draft.chunk_concurrency">
                <option v-for="n in SEGMENTS" :key="n" :value="n">{{ n }} 段</option>
              </select>
              <p class="note">
                同一个文件同时拉取的分段数：越大越快，也越容易碰到 B 站限速；选 1 就是顺序下载（最稳）。
                实际并发还要乘以「下载」页的「同时下载任务数」。
              </p>
            </div>

            <div class="field full">
              <label>FFmpeg 路径</label>
              <div class="row-flex">
                <input
                  :value="ffmpegValue"
                  spellcheck="false"
                  placeholder="留空时自动发现系统或常见包管理器中的 ffmpeg"
                  @input="ffmpegDraft = $event.target.value; draft.ffmpeg_path = $event.target.value"
                  @keydown.enter="ffmpegDraft = ''"
                />
                <button class="ghost" @click="chooseFfmpeg">选择</button>
              </div>
            </div>
          </div>

          <!-- 应用更新 -->
          <div v-else-if="active === 'update'" class="fields">
            <div class="field full">
              <label>自动检测</label>
              <p class="note top">
                默认关闭。开启后会在应用启动时静默检测新版本，不会自动下载或安装。
              </p>
              <label class="switch">
                <input type="checkbox" v-model="draft.update_check" />
                <span class="track"><span class="knob"></span></span>
              </label>
            </div>

            <div class="field full">
              <label>当前版本</label>
              <p class="version num">v{{ draft.version_placeholder || env?.version || "—" }}</p>
              <button class="ghost" :disabled="checkingUpdate" @click="checkUpdate">
                {{ checkingUpdate ? "检测中…" : "检测更新" }}
              </button>
              <p v-if="updateResult" class="note top">
                <template v-if="updateResult.error">无法检测：{{ updateResult.error }}</template>
                <template v-else-if="updateResult.up_to_date">已是最新版本（{{ updateResult.current }}）</template>
                <template v-else>
                  发现新版本 v{{ updateResult.latest }}，可到 Releases 页面下载。
                </template>
              </p>
            </div>

            <p class="note top dim">
              更新包通过 GitHub Releases 分发；仓库转公开或接入更新服务后，此处即可在线升级。
            </p>
          </div>

          <!-- 网络与维护 -->
          <div v-else class="fields">
            <div class="field full">
              <label>代理地址</label>
              <input
                v-model="proxyValue"
                spellcheck="false"
                placeholder="例如 http://127.0.0.1:7890，留空为直连"
                @keydown.enter="commitProxy"
              />
            </div>

            <div class="field full">
              <label>任务日志级别</label>
              <select v-model="draft.log_level">
                <option v-for="item in LOG_LEVELS" :key="item.value" :value="item.value">
                  {{ item.label }}
                </option>
              </select>
            </div>

            <div class="field full">
              <label>数据目录</label>
              <div class="row-flex">
                <input
                  :value="dataDirValue"
                  spellcheck="false"
                  placeholder="留空时使用默认数据目录"
                  @input="dataDirDraft = $event.target.value; draft.data_dir = $event.target.value"
                />
                <button class="ghost" @click="chooseDataDir">选择</button>
              </div>
              <p class="note">日志立即写入新目录；任务库与登录凭据的迁移将在后续版本支持。</p>
            </div>

            <div class="btn-row full">
              <button class="ghost" :disabled="cleaning === 'cache'" @click="cleanup('cache')">
                清理缓存
              </button>
              <button class="ghost" :disabled="cleaning === 'temp'" @click="cleanup('temp')">
                清理临时文件
              </button>
              <button class="ghost" :disabled="cleaning === 'diag'" @click="cleanup('diag')">
                导出诊断
              </button>
            </div>
          </div>
        </div>
      </div>

      <p v-else class="loading">正在读取设置…</p>
    </section>
  </div>
</template>

<style scoped>
.card {
  padding: 18px 20px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius-lg);
}

.head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding-bottom: 14px;
  border-bottom: 1px solid var(--line);
}

h1 {
  margin: 0;
  font-size: 22px;
  font-weight: 700;
  letter-spacing: -0.2px;
}

.spacer {
  flex: 1;
}

.primary {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 8px 20px;
  font-weight: 600;
  color: #fff;
  background: var(--accent);
  border-radius: var(--radius-sm);
}

.primary:hover:not(:disabled) {
  background: var(--accent-dark);
}

.primary:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.primary svg {
  width: 18px;
  height: 18px;
}

.ghost {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 13px;
  font-size: 12.5px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.ghost svg {
  width: 17px;
  height: 17px;
  color: var(--muted);
}

.ghost:hover:not(:disabled) {
  border-color: var(--accent-line);
  background: var(--raised);
}

.ghost:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.layout {
  display: flex;
  gap: 22px;
  margin-top: 18px;
  min-height: 420px;
}

/* 分类导航 */
.cats {
  flex: none;
  width: 190px;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.cats button {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 9px 11px;
  text-align: left;
  color: var(--muted);
  border: 1px solid transparent;
  border-radius: var(--radius);
  transition: background 0.15s ease, color 0.15s ease;
}

.cats button:hover {
  background: var(--raised);
  color: var(--text);
}

.cats button.active {
  color: var(--accent-dark);
  background: var(--accent-soft);
  border-color: var(--accent-line);
}

.icon {
  flex: none;
  width: 22px;
  height: 22px;
}

.cats button.active .icon {
  color: var(--accent);
}

.text {
  display: flex;
  flex-direction: column;
  line-height: 1.3;
}

.label {
  font-size: 13px;
  font-weight: 600;
}

.cats .hint {
  font-size: 11px;
  color: var(--faint);
}

/* 面板 */
.panel {
  flex: 1;
  min-width: 0;
}

.panel-head {
  display: flex;
  align-items: center;
  gap: 11px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--line);
}

.panel-icon {
  flex: none;
  display: grid;
  place-items: center;
  width: 38px;
  height: 38px;
  color: var(--accent);
  background: var(--accent-soft);
  border-radius: var(--radius-sm);
}

h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
}

.panel-hint {
  margin: 1px 0 0;
  font-size: 11.5px;
  color: var(--faint);
}

.fields {
  padding-top: 14px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.field label {
  font-size: 12.5px;
  font-weight: 600;
}

/* 三个一排（视频/音频/图片格式） */
.grid3 {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px;
}

.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}

.full {
  width: 100%;
}

.row-flex {
  display: flex;
  gap: 8px;
  align-items: center;
}

.row-flex input:first-child {
  flex: 1;
}

input:not([type="checkbox"]),
select {
  padding: 8px 11px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  font-size: 13px;
  transition: border-color 0.15s ease;
}

input:hover,
select:hover {
  border-color: var(--accent-line);
}

input:focus,
select:focus {
  outline: none;
  border-color: var(--accent-line);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

input::placeholder {
  color: var(--faint);
}

.note {
  margin: 0;
  font-size: 11.5px;
  color: var(--faint);
  line-height: 1.6;
}

/* 提醒类说明：用警告色，和普通说明区分开 */
.note.warn {
  color: var(--warn);
}

.note.top {
  margin-top: 4px;
}

.note.dim {
  color: var(--faint);
  opacity: 0.85;
}

.token-hint {
  font-size: 11px;
  color: var(--faint);
  white-space: nowrap;
}

/* 变量插入：按钮 + 面板共用定位上下文 */
.var-picker {
  position: relative;
  flex: none;
}

.var-picker .ghost {
  padding: 8px 10px;
}

.var-picker .ghost.on {
  color: var(--accent);
  border-color: var(--accent-line);
}

.var-panel {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 15;
  /* 四列横排要的宽度；窗口窄的时候收一收，别顶到侧栏外面去 */
  width: min(644px, calc(100vw - 260px));
  padding: 10px;
  text-align: left;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  box-shadow: 0 12px 30px rgba(20, 12, 16, 0.24);
}

.var-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 2px 4px 8px;
  font-size: 12.5px;
}

.var-head span {
  font-size: 11px;
  color: var(--faint);
}

/* 栏目横排：一列一个栏目，列内变量竖着堆。
   四列刚好放下八个栏目两行，19 个变量一次看全，不用滚。 */
.var-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px 12px;
  align-items: start;
}

.var-col {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

/* 栏目名：就是列标题，压住一列的顶 */
.var-col-name {
  margin: 0 0 3px;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--line);
  font-size: 11.5px;
  font-weight: 600;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.var-item {
  display: flex;
  flex-direction: column;
  min-width: 0;
  padding: 4px 7px;
  line-height: 1.25;
  text-align: left;
  border-radius: var(--radius-sm);
}

.var-item:hover {
  background: var(--hover);
}

.var-item code {
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  font-size: 11.5px;
  line-height: 1.25;
  color: var(--accent);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.var-item span {
  font-size: 10.5px;
  line-height: 1.25;
  color: var(--faint);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.checks {
  display: flex;
  gap: 22px;
  flex-wrap: wrap;
}

.check {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  cursor: pointer;
}

/* 勾选框外观统一在 styles.css 里定义，这里只管布局 */
.check input {
  flex: none;
}

.card-check {
  padding: 11px 13px;
  background: var(--raised);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius-sm);
}

.group {
  padding: 13px 14px;
  background: var(--raised);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius);
}

.group-title {
  margin-bottom: 10px;
  font-size: 12.5px;
  font-weight: 700;
}

/* 提示图标：ASCII 的 ? + CSS 圆圈。
   原来用 ⓘ 字符，WebView2 落到的字体把它渲染成"圆圈里一根竖条"，看着像坏图标；
   字形落哪套字体不可控，不如自己画。 */
.info {
  display: inline-grid;
  place-items: center;
  width: 15px;
  height: 15px;
  font-size: 9.5px;
  font-weight: 700;
  line-height: 1;
  color: var(--faint);
  border: 1px solid currentColor;
  border-radius: 50%;
  vertical-align: 0.5px;
  cursor: help;
}

.info:hover {
  color: var(--accent);
}

.sub-card {
  padding: 13px 14px;
  background: var(--raised);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius);
}

.sub-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
}

/* 卡片头上的小图标：发丝描边的方片，和侧栏分类图标同一个语言 */
.sub-icon {
  flex: none;
  width: 30px;
  height: 30px;
  padding: 5px;
  color: var(--accent-ink);
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.sub-text {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.sub-title {
  font-size: 12.5px;
  font-weight: 700;
}

.sub-note {
  font-size: 11.5px;
  color: var(--faint);
}

/* 优先顺序列表：一条 = 序号 + 取值 + 上移/下移/删除 */
.pref-row {
  display: flex;
  align-items: flex-end;
  gap: 10px;
  padding: 9px 10px;
  background: var(--card);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius-sm);
}

.pref-row + .pref-row {
  margin-top: 7px;
}

.pref-no {
  flex: none;
  width: 18px;
  padding-bottom: 9px;
  font-size: 12px;
  color: var(--faint);
  text-align: center;
}

.pref-row .field {
  flex: 1;
  min-width: 0;
}

.pref-row .field + .field {
  margin-left: 2px;
}

.pref-acts {
  display: flex;
  flex: none;
  gap: 2px;
  padding-bottom: 2px;
}

.act {
  width: 26px;
  height: 30px;
  font-size: 13px;
  color: var(--muted);
  border-radius: var(--radius-sm);
}

.act:hover:not(:disabled) {
  color: var(--text);
  background: var(--hover);
}

.act:disabled {
  color: var(--line);
  cursor: not-allowed;
}

/* 运行环境块 */
.env {
  padding: 13px 14px;
  background: var(--raised);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius);
}

.env-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
}

.env-title {
  font-size: 12.5px;
  font-weight: 700;
}

.env-title b {
  color: var(--accent-dark);
}

.env-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
}

.env-name {
  font-weight: 600;
}

.env-info {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--faint);
}

.badge {
  padding: 1px 9px;
  font-size: 11px;
  font-weight: 600;
  border-radius: 999px;
}

.badge.ok {
  color: var(--ok);
  background: color-mix(in srgb, var(--ok) 14%, transparent);
}

.badge.bad {
  color: var(--err);
  background: color-mix(in srgb, var(--err) 14%, transparent);
}

.badge.small {
  font-size: 10.5px;
}

.btn-row {
  display: flex;
  gap: 10px;
}

/* 开关 */
.switch {
  display: inline-flex;
  align-items: center;
  cursor: pointer;
  margin-top: 2px;
}

.switch input {
  display: none;
}

.switch .track {
  position: relative;
  width: 38px;
  height: 21px;
  background: var(--seg-idle);
  border-radius: 999px;
  transition: background 0.15s ease;
}

.switch .knob {
  position: absolute;
  top: 2.5px;
  left: 3px;
  width: 19px;
  height: 19px;
  background: #fff;
  border-radius: 50%;
  transition: transform 0.15s ease;
}

.switch input:checked + .track {
  background: var(--accent);
}

.switch input:checked + .track .knob {
  transform: translateX(16px);
}

.version {
  margin: 0;
  font-size: 17px;
  font-weight: 700;
}

.loading {
  margin: 22px 0 0;
  font-size: 12.5px;
  color: var(--faint);
}
</style>
