<script setup>
import { computed, ref } from "vue";
import * as api from "../api";

const props = defineProps({
  login: { type: Object, required: true },
  /** 已保存的设置（后端结构） */
  settings: { type: Object, default: null },
  env: { type: Object, default: null },
});
const emit = defineEmits(["toast", "save", "reset", "reload", "login", "logout"]);

const CATEGORY_ICONS = {
  download: "M12 4.8v9.6M8.4 10.8 12 14.4l3.6-3.6M5.6 18.4h12.8",
  media: "M4.6 7.4h14.8v9.2H4.6zM9.8 10v4l3.6-2-3.6-2Z",
  naming: "M7 4.6h7l4 4v10.8H7zM14 4.6V9h4M9.4 13h5.2M9.4 16.4h5.2",
  encode: "M8.4 4.8v14.4M8.4 19.2 5.2 16M15.6 4.8v14.4M15.6 4.8 12.4 8",
  extras: "M6.4 5.4h11.2v13.2H6.4zM9.4 9.4h5.2M9.4 12.6h5.2M9.4 15.8h3",
  update: "M19.4 12a7.4 7.4 0 1 1-2.2-5.2M19.4 4.6v4h-4",
  network: "M12 19.4a7.4 7.4 0 1 0 0-14.8 7.4 7.4 0 0 0 0 14.8ZM3.6 12h16.8M12 4.6c-4.4 4.4-4.4 10.4 0 14.8 4.4-4.4 4.4-10.4 0-14.8Z",
};

const categories = [
  { key: "download", label: "下载", hint: "目录、并发与恢复" },
  { key: "media", label: "媒体", hint: "清晰度与封装格式" },
  { key: "naming", label: "文件命名", hint: "模板与重名处理" },
  { key: "encode", label: "编码与处理", hint: "编码、分段与 FFmpeg" },
  { key: "extras", label: "附加内容", hint: "封面、字幕与弹幕" },
  { key: "update", label: "应用更新", hint: "版本检测与安装" },
  { key: "network", label: "网络与维护", hint: "代理、日志与数据" },
];

const active = ref("download");
const activeCategory = computed(() =>
  categories.find((category) => category.key === active.value)
);

/** 本地草稿：编辑期间不落盘，点「保存」才提交 */
const draft = ref(null);
const BUILTIN_PRESETS = [
  { name: "默认", template: "{title}" },
  { name: "标题_BV号", template: "{title}_{bvid}" },
  { name: "标题_清晰度", template: "{title}_{quality}" },
  { name: "标题_UP主", template: "{title}_{owner}" },
  { name: "完整信息", template: "{title}_{quality}_{bvid}" },
];

const selectedPreset = ref("");
const presetName = ref("");

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

const QUALITIES = [
  { value: 0, label: "最优画质" },
  { value: 127, label: "8K" },
  { value: 126, label: "杜比视界" },
  { value: 125, label: "HDR" },
  { value: 120, label: "4K" },
  { value: 116, label: "1080P60" },
  { value: 112, label: "1080P+" },
  { value: 80, label: "1080P" },
  { value: 64, label: "720P" },
  { value: 32, label: "480P" },
];

const AUDIOS = [
  { value: "auto", label: "最佳可用" },
  { value: "normal", label: "普通音轨" },
  { value: "dolby", label: "杜比全景声" },
  { value: "flac", label: "Hi-Res 无损优先" },
];

const CONTAINERS = [
  { value: "mp4", label: "MP4" },
  { value: "mkv", label: "MKV" },
];

const CODECS = [
  { value: "auto", label: "自动" },
  { value: "avc", label: "优先 AVC（兼容性最好）" },
  { value: "hevc", label: "优先 HEVC（压缩率更高）" },
];

const FALLBACKS = [
  { value: "nearest", label: "选择接近的可用质量" },
  { value: "fail", label: "任务失败并提示" },
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

const namingPreview = computed(() => {
  const tpl = draft.value?.naming_template || "{title}";
  const render = (token) =>
    ({ title: "示例视频", bvid: "BV1Vkag6TExf", quality: "1080P60", owner: "示例UP主" })[token] ??
    `{{{token}}}`;
  let out = "";
  let rest = tpl;
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
    out += render(rest.slice(start + 1, end));
    rest = rest.slice(end + 1);
  }
  const ext = draft.value?.container === "mkv" ? "mkv" : "mp4";
  return `${out}.${ext}`;
});

const parsePaceNote = computed(() => {
  const batch = draft.value?.parse_batch ?? 8;
  const wait = draft.value?.parse_batch_wait_ms ?? 1000;
  const every = draft.value?.parse_rest_every ?? 100;
  const rest = draft.value?.parse_rest_ms ?? 3000;
  const total = 200;
  const batches = Math.ceil(total / batch);
  const extraMs = Math.max(batches - 1, 0) * wait + Math.floor(total / every) * rest;
  return `解析约 ${total} 条，额外等待约 ${Math.round(extraMs / 1000)} 秒，不含网络耗时。`;
});

function set(key, value) {
  if (draft.value) draft.value[key] = value;
}

/** 选预设：内置或用户保存的，选中即填入模板 */
function selectPreset(name) {
  selectedPreset.value = name;
  if (!name || !draft.value) return;
  const builtin = BUILTIN_PRESETS.find((preset) => preset.name === name);
  if (builtin) {
    draft.value.naming_template = builtin.template;
    return;
  }
  const saved = draft.value.naming_presets?.find((preset) => preset.name === name);
  if (saved) draft.value.naming_template = saved.template;
}

/** 保存为预设：同名更新模板，随设置一起落盘 */
function savePreset() {
  const name = presetName.value.trim();
  if (!name || !draft.value) {
    emit("toast", "先填写预设名称，再保存为预设");
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
  selectedPreset.value = name;
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
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path
              d="M5.2 12a6.8 6.8 0 1 1 2 4.8M5.2 17v-4.4h4.4"
              fill="none"
              stroke="currentColor"
              stroke-width="1.6"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
          恢复默认
        </button>
        <button class="ghost" :disabled="!dirty" @click="undo">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path
              d="M18.8 12a6.8 6.8 0 1 1-2-4.8M18.8 7v4.4h-4.4"
              fill="none"
              stroke="currentColor"
              stroke-width="1.6"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
          撤销
        </button>
        <button class="primary" :disabled="!dirty" @click="save">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path
              d="M6.4 5.4h9.2l3 3v10.2H6.4zM9.4 5.4v3.6h5.2M9 13.6h6"
              fill="none"
              stroke="currentColor"
              stroke-width="1.6"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
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
            <svg class="icon" viewBox="0 0 24 24" aria-hidden="true">
              <path
                :d="CATEGORY_ICONS[category.key]"
                fill="none"
                stroke="currentColor"
                stroke-width="1.6"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
            <span class="text">
              <span class="label">{{ category.label }}</span>
              <span class="hint">{{ category.hint }}</span>
            </span>
          </button>
        </nav>

        <div class="panel">
          <div class="panel-head">
            <svg class="panel-icon" viewBox="0 0 24 24" aria-hidden="true">
              <path
                :d="CATEGORY_ICONS[activeCategory.key]"
                fill="none"
                stroke="currentColor"
                stroke-width="1.6"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
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
                <span class="info" title="批量解析时按此节奏分批请求，降低触发风控的概率">ⓘ</span>
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
                  <label>每批解析 <span class="info">ⓘ</span></label>
                  <select v-model.number="draft.parse_batch">
                    <option :value="3">3 条</option>
                    <option :value="8">8 条</option>
                    <option :value="16">16 条</option>
                    <option :value="30">30 条</option>
                  </select>
                </div>
                <div class="field">
                  <label>批间等待 <span class="info">ⓘ</span></label>
                  <select v-model.number="draft.parse_batch_wait_ms">
                    <option :value="500">0.5 秒</option>
                    <option :value="1000">1 秒</option>
                    <option :value="2000">2 秒</option>
                    <option :value="3000">3 秒</option>
                  </select>
                </div>
                <div class="field">
                  <label>每 {{ draft.parse_rest_every }} 条休息 <span class="info">ⓘ</span></label>
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
                  <option v-for="item in QUALITIES" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>音频质量</label>
                <select v-model="draft.default_audio">
                  <option v-for="item in AUDIOS" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </div>
            </div>

            <div class="field full">
              <label>封装格式</label>
              <select v-model="draft.container">
                <option v-for="item in CONTAINERS" :key="item.value" :value="item.value">
                  {{ item.label }}
                </option>
              </select>
              <p class="note">嵌入封面和字幕时需使用 MKV</p>
            </div>

            <div class="sub-card full">
              <div class="sub-head">
                <span class="sub-title">画质优先顺序</span>
                <span class="spacer"></span>
              </div>
              <div class="field">
                <label>视频编码偏好</label>
                <select v-model="draft.codec_pref">
                  <option v-for="item in CODECS" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </div>
              <p class="note">同一清晰度有多个编码时按此偏好选择；不会为编码牺牲清晰度。</p>
            </div>

            <div class="sub-card full">
              <div class="sub-head">
                <span class="sub-title">音频优先顺序</span>
                <span class="spacer"></span>
              </div>
              <div class="field">
                <label>音轨策略</label>
                <select v-model="draft.default_audio">
                  <option value="auto">自动（最佳可用）</option>
                  <option value="flac">优先 Hi-Res 无损</option>
                  <option value="dolby">优先杜比全景声</option>
                  <option value="normal">仅普通音轨</option>
                </select>
              </div>
              <p class="note">所选音轨不存在时自动退回普通音轨。</p>
            </div>
          </div>

          <!-- 文件命名 -->
          <div v-else-if="active === 'naming'" class="fields">
            <div class="field full">
              <label>命名预设</label>
              <select :value="selectedPreset" @change="selectPreset($event.target.value)">
                <option value="">自定义…</option>
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
                <input v-model="draft.naming_template" spellcheck="false" />
                <span class="token-hint">
                  可用标记：{title} {bvid} {quality} {owner}
                </span>
              </div>
              <p class="note">
                文件名预览：<b>{{ namingPreview }}</b>
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
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path
                      d="M6.4 5.4h9.2l3 3v10.2H6.4zM9.4 5.4v3.6h5.2"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="1.6"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    />
                  </svg>
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

          <!-- 编码与处理 -->
          <div v-else-if="active === 'encode'" class="fields">
            <div class="grid2">
              <div class="field">
                <label>视频编码偏好</label>
                <select v-model="draft.codec_pref">
                  <option v-for="item in CODECS" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>目标质量不可用</label>
                <select v-model="draft.quality_fallback">
                  <option v-for="item in FALLBACKS" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </div>
            </div>

            <div class="field full">
              <label>单任务分段数</label>
              <select v-model.number="draft.chunk_concurrency">
                <option v-for="n in SEGMENTS" :key="n" :value="n">{{ n }} 段</option>
              </select>
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

          <!-- 附加内容 -->
          <div v-else-if="active === 'extras'" class="fields">
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

            <label class="check card-check full">
              <input type="checkbox" v-model="draft.keep_temp" disabled />
              <span>保留原始视频/音频频道（由上方「下载范围」控制）</span>
            </label>

            <div class="grid2 full">
              <label class="check card-check">
                <input type="checkbox" v-model="draft.embed_cover" />
                <span>嵌入封面（仅 MKV）</span>
              </label>
              <label class="check card-check" :title="draft.container === 'mp4' ? '请先将封装格式切换为 MKV' : ''">
                <input type="checkbox" v-model="draft.embed_subtitles" />
                <span>嵌入字幕（仅 MKV）</span>
              </label>
            </div>
            <p v-if="draft.embed_subtitles" class="note">
              字幕与弹幕下载将在后续版本提供，当前仅保存该选项。
            </p>
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
  width: 15px;
  height: 15px;
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
  width: 14px;
  height: 14px;
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
  width: 18px;
  height: 18px;
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
  width: 34px;
  height: 34px;
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

.check input {
  width: 15px;
  height: 15px;
  accent-color: var(--accent);
}

.check input:disabled {
  cursor: not-allowed;
  opacity: 0.5;
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

.info {
  color: var(--faint);
  font-weight: 400;
  cursor: help;
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

.sub-title {
  font-size: 12.5px;
  font-weight: 700;
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
  width: 16px;
  height: 16px;
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
