<script setup>
import { computed, ref } from "vue";
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
  space: "UP 空间",
  bangumi: "番剧",
  cheese: "课程",
};

/** 批量来源里每条的勾选状态 */
const selected = ref(new Set());
const batchQuality = ref(80);
const batchAudio = ref("normal");

const okItems = computed(() => items.value.filter((item) => item.ok));
const batchProbe = computed(() => {
  const first = okItems.value.find((item) => item.probe?.kind !== "video");
  return first ? first.probe : null;
});
const batchSelected = computed(() => {
  const batch = batchProbe.value;
  if (!batch) return [];
  const keyOf = (entry) => `${batch.kind}:${entry.bvid || `ep-${entry.ep_id}`}`;
  return batch.items.filter((entry) => selected.value.has(keyOf(entry))).map(keyOf);
});
const isBatch = computed(() => !!batchProbe.value);

function itemKey(item) {
  return item.probe ? `${item.probe.kind}:${item.bvid || `ep-${item.epId}`}` : item.input;
}
function hasKey(key) {
  return okItems.value.some((item) => itemKey(item) === key);
}

const steps = computed(() => {
  const batch = batchProbe.value;
  return [
    {
      index: 1,
      title: "解析来源",
      hint: parsing.value ? `解析中 ${done.value}/${total.value}` : "输入链接",
      state: items.value.length && !parsing.value ? "done" : "active",
    },
    {
      index: 2,
      title: "选择内容",
      hint: items.value.length
        ? isBatch.value
          ? `已选中 ${batchSelected.value.length} / ${batch.loaded}`
          : `已解析 ${okItems.value.length} 条`
        : "等待解析",
      state: items.value.length && !parsing.value ? "active" : "idle",
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

function lines() {
  return text.value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
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
  const inputs = lines();
  if (!inputs.length || parsing.value) return;

  // 按解析节奏分批：批内并发、批间等待、每 N 条休息，降低触发风控的概率
  parsing.value = true;
  items.value = [];
  selected.value = new Set();
  done.value = 0;
  total.value = inputs.length;

  const collected = [];
  const batch = Math.max(1, props.settings?.parse_batch ?? 8);
  const batchWait = props.settings?.parse_batch_wait_ms ?? 1000;
  const restEvery = Math.max(1, props.settings?.parse_rest_every ?? 100);
  const restMs = props.settings?.parse_rest_ms ?? 3000;

  const probeOne = async (input) => {
    try {
      const probe = await api.probeSource(input);
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
        probe.items.forEach((item) =>
          selected.value.add(`${probe.kind}:${item.bvid || `ep-${item.ep_id}`}`)
        );
        batchQuality.value = pickDefaultQuality(probe);
        batchAudio.value = pickDefaultAudio(probe);
        selected.value = new Set(selected.value);
        collected.push({ input, ok: true, error: "", probe });
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
  const failed = collected.filter((item) => !item.ok).length;
  if (failed) {
    emit("toast", `${collected.length - failed} 条解析成功，${failed} 条失败`);
  }
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

async function startBatch(batch) {
  const source = batch.kind;
  let started = 0;
  // 列表序号从 1 开始，供命名模板的 {index} / {episode_index} 使用
  let position = 0;
  for (const entry of batch.items) {
    position += 1;
    const key = `${batch.kind}:${entry.bvid || `ep-${entry.ep_id}`}`;
    if (!selected.value.has(key)) continue;
    try {
      await api.startDownload({
        bvid: entry.bvid,
        cid: entry.cid,
        ep_id: entry.ep_id,
        source,
        title: entry.title,
        owner: batch.owner,
        quality: batchQuality.value,
        audio: batchAudio.value,
        cover: "",
        naming: batchNaming(batch, entry, position),
      });
      started += 1;
    } catch (error) {
      emit("toast", `${entry.title}: ${error}`);
    }
  }
  return started;
}

async function startAll() {
  let started = 0;

  for (const item of okItems.value) {
    if (item.probe.kind === "video") {
      if (await startSingle(item)) started += 1;
    }
  }
  if (batchProbe.value) {
    started += await startBatch(batchProbe.value);
  }

  if (started) {
    emit("toast", `已加入 ${started} 个下载任务`);
    items.value = [];
    selected.value = new Set();
    text.value = "";
    emit("goto", "transfer");
  } else {
    emit("toast", "没有勾选任何内容，先在列表里勾选要下载的条目");
  }
}
</script>

<template>
  <div>
    <StepHeader :steps="steps" />

    <section class="card">
      <h1>解析链接</h1>
      <p class="lead">粘贴一个或多个 Bilibili 来源，解析后再选择要下载的内容与清晰度。</p>

      <div class="tabs" role="tablist">
        <button :class="{ active: mode === 'batch' }" role="tab" @click="mode = 'batch'">
          批量解析
        </button>
        <button :class="{ active: mode === 'single' }" role="tab" @click="mode = 'single'">
          单个视频
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
        每行一个来源；支持 BV 号、av 号、完整链接、b23.tv 短链，以及收藏夹 / 合集 / UP
        空间 / 番剧 / 课程链接。
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

      <div class="sources">
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

    <section v-if="items.length" class="card results">
      <header class="results-head">
        <h2>选择内容</h2>
        <span class="count num">{{ batchSelected.length || okItems.length }}</span>
        <span class="spacer"></span>
        <button class="ghost" @click="items = []">清空</button>
        <button class="primary" :disabled="!okItems.length" @click="startAll">
          全部加入下载
        </button>
      </header>

      <ul class="list">
        <li
          v-for="(item, index) in items"
          :key="item.input + index"
          class="item"
          :class="{ failed: !item.ok }"
        >
          <template v-if="item.ok && item.probe.kind === 'video'">
            <div class="thumb">
              <img v-if="item.probe.cover" :src="item.probe.cover" alt="" />
              <svg v-else class="thumb-placeholder" viewBox="0 0 24 24" aria-hidden="true">
                <rect
                  x="3.2"
                  y="5.6"
                  width="17.6"
                  height="12.8"
                  rx="2.4"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.5"
                />
                <path d="M10.4 9.6v4.8l4.3-2.4-4.3-2.4Z" fill="currentColor" />
              </svg>
            </div>
            <div class="meta">
              <p class="title">{{ item.probe.title }}</p>
              <p class="sub">
                <span>{{ item.probe.owner }}</span>
                <span class="num">{{ formatDuration(item.probe.duration) }}</span>
                <span class="num faint">{{ item.probe.bvid }}</span>
              </p>
              <p v-if="item.probe.note" class="note">{{ item.probe.note }}</p>
            </div>
            <label class="field">
              <span>清晰度</span>
              <select v-model.number="item.quality">
                <option
                  v-for="quality in item.probe.qualities"
                  :key="quality.qn"
                  :value="quality.qn"
                  :disabled="!quality.available"
                >
                  {{ quality.label }}{{ quality.hint ? `（${quality.hint}）` : "" }}
                </option>
              </select>
            </label>
            <label class="field">
              <span>音轨</span>
              <select v-model="item.audio">
                <option
                  v-for="audio in item.probe.audios"
                  :key="audio.kind"
                  :value="audio.kind"
                  :disabled="!audio.available"
                >
                  {{ audio.label }}{{ audio.available ? "" : "（不可用）" }}
                </option>
              </select>
            </label>
            <button class="remove" title="移除" @click="items.splice(index, 1)">✕</button>
          </template>

          <template v-else-if="item.ok">
            <!-- 批量来源：收藏夹 / 合集 / UP 空间 / 番剧 / 课程 -->
            <div class="batch">
              <div class="batch-head">
                <span class="kind">{{ KIND_LABELS[item.probe.kind] }}</span>
                <div class="meta">
                  <p class="title">{{ item.probe.title }}</p>
                  <p class="sub">
                    <span v-if="item.probe.owner">{{ item.probe.owner }}</span>
                    <span class="num">
                      {{ item.probe.loaded }} 条{{ item.probe.note ? ` · ${item.probe.note}` : "" }}
                    </span>
                  </p>
                  <p v-if="item.probe.note" class="note">{{ item.probe.note }}</p>
                </div>
                <label class="field">
                  <span>清晰度（应用到全部）</span>
                  <select v-model.number="batchQuality">
                    <option
                      v-for="quality in item.probe.qualities"
                      :key="quality.qn"
                      :value="quality.qn"
                      :disabled="!quality.available"
                    >
                      {{ quality.label }}{{ quality.hint ? `（${quality.hint}）` : "" }}
                    </option>
                  </select>
                </label>
                <label class="field">
                  <span>音轨</span>
                  <select v-model="batchAudio">
                    <option
                      v-for="audio in item.probe.audios"
                      :key="audio.kind"
                      :value="audio.kind"
                      :disabled="!audio.available"
                    >
                      {{ audio.label }}{{ audio.available ? "" : "（不可用）" }}
                    </option>
                  </select>
                </label>
                <button class="remove" title="移除" @click="items.splice(index, 1)">✕</button>
              </div>

              <div class="batch-tools">
                <button class="mini" @click="selectAll">全选</button>
                <button class="mini" @click="selectNone">全不选</button>
                <span class="hint num">
                  已选 {{ batchSelected.length }} / {{ item.probe.loaded }}
                </span>
              </div>

              <ul class="batch-list">
                <li
                  v-for="(entry, entryIndex) in item.probe.items"
                  :key="entry.bvid || entry.ep_id"
                >
                  <label class="check">
                    <input
                      type="checkbox"
                      :checked="selected.has(`${item.probe.kind}:${entry.bvid || `ep-${entry.ep_id}`}`)"
                      @change="toggleItem(`${item.probe.kind}:${entry.bvid || `ep-${entry.ep_id}`}`)"
                    />
                    <span class="idx num">{{ entryIndex + 1 }}</span>
                    <span class="entry-title" :title="entry.title">{{ entry.title }}</span>
                    <span class="entry-dur num">{{ formatDuration(entry.duration) }}</span>
                  </label>
                </li>
              </ul>
            </div>
          </template>

          <template v-else>
            <div class="meta">
              <p class="title">{{ item.input }}</p>
              <p class="error">{{ item.error }}</p>
            </div>
            <button class="remove" title="移除" @click="items.splice(index, 1)">✕</button>
          </template>
        </li>
      </ul>

      <p v-if="!login.logged_in && okItems.length" class="login-tip">
        未登录最高只能下载 480P，点击右上角完成登录可解锁 1080P 及以上。
      </p>
    </section>
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
  margin: 20px 0 14px;
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
  border-color: #ded6da;
}

.tabs button.active {
  color: var(--accent-dark);
  background: var(--accent-soft);
  border-color: var(--accent-line);
  font-weight: 600;
}

textarea,
input {
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
  margin-top: 20px;
  padding-top: 15px;
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

/* 解析结果 */
.results {
  margin-top: 16px;
}

.results-head {
  display: flex;
  align-items: center;
  gap: 9px;
  margin-bottom: 14px;
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

.batch-list input {
  width: 14px;
  height: 14px;
  accent-color: var(--accent);
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
</style>
