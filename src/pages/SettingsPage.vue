<script setup>
import { computed, ref } from "vue";
import * as api from "../api";

const props = defineProps({
  login: { type: Object, required: true },
  settings: { type: Object, default: null },
  env: { type: Object, default: null },
});
const emit = defineEmits(["toast", "login", "logout", "change", "reload", "reset"]);

const CATEGORY_ICONS = {
  download: "M12 4.8v9.6M8.4 10.8 12 14.4l3.6-3.6M5.6 18.4h12.8",
  media: "M4.6 7.4h14.8v9.2H4.6zM9.8 10v4l3.6-2-3.6-2Z",
  network: "M12 19.4a7.4 7.4 0 1 0 0-14.8 7.4 7.4 0 0 0 0 14.8ZM3.6 12h16.8M12 4.6c-4.4 4.4-4.4 10.4 0 14.8 4.4-4.4 4.4-10.4 0-14.8Z",
  maintain: "M12 20.2a8.2 8.2 0 1 0 0-16.4 8.2 8.2 0 0 0 0 16.4ZM12 11v5.4M12 7.6v.9",
};

const categories = [
  { key: "download", label: "下载", hint: "目录、并发与分片" },
  { key: "media", label: "媒体", hint: "清晰度、音轨与命名" },
  { key: "network", label: "网络", hint: "代理与直连" },
  { key: "maintain", label: "账号与维护", hint: "账号、凭据与运行环境" },
];

const active = ref("download");
const activeCategory = computed(() =>
  categories.find((category) => category.key === active.value)
);

const QUALITIES = [
  { value: 0, label: "最优画质（自动）" },
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
  { value: "normal", label: "最佳可用" },
  { value: "dolby", label: "杜比全景声" },
  { value: "flac", label: "Hi-Res 无损" },
];

const NAMINGS = [
  { value: "title", label: "标题" },
  { value: "title_quality", label: "标题_清晰度" },
  { value: "title_bvid", label: "标题_BV号" },
];

const CONCURRENCY = [1, 2, 3, 4, 5];
const CHUNK_CONCURRENCY = [1, 2, 4, 6, 8, 12, 16];
const CHUNK_MB = [1, 2, 4, 8, 16, 32];

const proxyDraft = ref("");
const proxyValue = computed({
  get: () =>
    proxyDraft.value !== "" ? proxyDraft.value : props.settings?.proxy ?? "",
  set: (value) => (proxyDraft.value = value),
});

const namingExample = computed(() => {
  const naming = props.settings?.naming ?? "title";
  if (naming === "title_quality") return "标题_1080P60.mp4";
  if (naming === "title_bvid") return "标题_BV1Vkag6TExf.mp4";
  return "标题.mp4";
});

function patch(key, value) {
  emit("change", { [key]: value });
}

function patchNumber(key, value) {
  emit("change", { [key]: Number(value) });
}

async function chooseDir() {
  try {
    emit("change", { output_dir: await api.chooseOutputDir() });
  } catch (error) {
    emit("toast", String(error));
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

function applyProxy() {
  patch("proxy", proxyValue.value.trim());
  proxyDraft.value = "";
}

function parentDir(path) {
  const index = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return index > 0 ? path.slice(0, index) : path;
}
</script>

<template>
  <div>
    <section class="card">
      <header class="head">
        <div>
          <h1>设置</h1>
          <p class="lead">改动即时保存。</p>
        </div>
        <span class="spacer"></span>
        <button class="ghost" @click="emit('reset')">
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
      </header>

      <div v-if="settings" class="layout">
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
          <dl v-if="active === 'download'" class="rows">
            <div class="row">
              <dt>保存位置</dt>
              <dd>
                <span class="path" :title="settings.output_dir">{{ settings.output_dir }}</span>
              </dd>
              <div class="ops">
                <button class="ghost" @click="chooseDir">选择目录</button>
                <button class="ghost" @click="open(settings.output_dir)">打开</button>
              </div>
            </div>
            <div class="row">
              <dt>同时下载</dt>
              <dd>
                <select
                  :value="settings.max_concurrent_tasks"
                  @change="patchNumber('max_concurrent_tasks', $event.target.value)"
                >
                  <option v-for="n in CONCURRENCY" :key="n" :value="n">{{ n }} 个任务</option>
                </select>
                <span class="hint">超出的任务排队等待</span>
              </dd>
            </div>
            <div class="row">
              <dt>分片</dt>
              <dd>
                <select
                  :value="settings.chunk_concurrency"
                  @change="patchNumber('chunk_concurrency', $event.target.value)"
                >
                  <option v-for="n in CHUNK_CONCURRENCY" :key="n" :value="n">并发 {{ n }}</option>
                </select>
                <select
                  :value="settings.chunk_mb"
                  @change="patchNumber('chunk_mb', $event.target.value)"
                >
                  <option v-for="n in CHUNK_MB" :key="n" :value="n">每片 {{ n }} MB</option>
                </select>
              </dd>
            </div>
            <div class="row">
              <dt>分轨文件</dt>
              <dd>
                <label class="check">
                  <input
                    type="checkbox"
                    :checked="settings.keep_temp"
                    @change="patch('keep_temp', $event.target.checked)"
                  />
                  <span>合成后保留音视频分轨（便于自查，会额外占用空间）</span>
                </label>
              </dd>
            </div>
          </dl>

          <!-- 媒体 -->
          <dl v-else-if="active === 'media'" class="rows">
            <div class="row">
              <dt>默认清晰度</dt>
              <dd>
                <select
                  :value="settings.default_quality"
                  @change="patchNumber('default_quality', $event.target.value)"
                >
                  <option v-for="item in QUALITIES" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
                <span class="hint">解析后预选这一档，该视频没有时自动退回其最高档</span>
              </dd>
            </div>
            <div class="row">
              <dt>默认音轨</dt>
              <dd>
                <select
                  :value="settings.default_audio"
                  @change="patch('default_audio', $event.target.value)"
                >
                  <option v-for="item in AUDIOS" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
              </dd>
            </div>
            <div class="row">
              <dt>文件命名</dt>
              <dd>
                <select :value="settings.naming" @change="patch('naming', $event.target.value)">
                  <option v-for="item in NAMINGS" :key="item.value" :value="item.value">
                    {{ item.label }}
                  </option>
                </select>
                <span class="hint">例：{{ namingExample }}</span>
              </dd>
            </div>
          </dl>

          <!-- 网络 -->
          <dl v-else-if="active === 'network'" class="rows">
            <div class="row">
              <dt>代理</dt>
              <dd class="grow">
                <input
                  v-model="proxyValue"
                  spellcheck="false"
                  placeholder="http://127.0.0.1:7890（留空为直连）"
                  @keydown.enter="applyProxy"
                />
                <span class="hint">改完按回车或点「应用」生效；下载走代理时填这里</span>
              </dd>
              <div class="ops">
                <button class="ghost" @click="applyProxy">应用</button>
              </div>
            </div>
          </dl>

          <!-- 账号与维护 -->
          <dl v-else class="rows">
            <div class="row">
              <dt>账号</dt>
              <dd>
                <template v-if="login.logged_in">
                  <span class="strong">{{ login.uname }}</span>
                  <span class="muted num">UID {{ login.mid }}</span>
                  <span v-if="login.vip" class="vip">{{ login.vip_label || "大会员" }}</span>
                </template>
                <span v-else class="muted">未登录（最高 480P）</span>
              </dd>
              <div class="ops">
                <button v-if="login.logged_in" class="ghost danger" @click="emit('logout')">
                  退出登录
                </button>
                <button v-else class="ghost" @click="emit('login')">扫码登录</button>
              </div>
            </div>
            <div class="row">
              <dt>登录凭据</dt>
              <dd>
                <span class="path" :title="env?.cookies_path">{{ env?.cookies_path || "—" }}</span>
                <span class="hint">
                  {{ env?.cookies_saved ? "已保存，等同账号密码，请勿分享" : "尚未保存" }}
                </span>
              </dd>
              <div class="ops">
                <button
                  class="ghost"
                  :disabled="!env?.cookies_path"
                  @click="open(parentDir(env.cookies_path))"
                >
                  打开目录
                </button>
              </div>
            </div>
            <div class="row">
              <dt>ffmpeg</dt>
              <dd>
                <span :class="env?.ffmpeg_ok ? 'strong' : 'warn'">
                  {{ env?.ffmpeg_ok ? "已就绪" : "不可用" }}
                </span>
                <span class="hint wrap">{{ env?.ffmpeg_info || "—" }}</span>
              </dd>
              <div class="ops">
                <button class="ghost" @click="emit('reload')">重新检测</button>
              </div>
            </div>
            <div class="row">
              <dt>版本</dt>
              <dd>
                <span class="num">BILIdown v{{ env?.version || "—" }}</span>
                <span class="hint">Tauri 2 · Rust · Vue 3</span>
              </dd>
              <div class="ops"></div>
            </div>
          </dl>
        </div>
      </div>

      <p v-else class="loading">正在读取设置…</p>
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

.head {
  display: flex;
  align-items: flex-start;
  gap: 12px;
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

.spacer {
  flex: 1;
}

.layout {
  display: flex;
  gap: 20px;
  margin-top: 20px;
  min-height: 380px;
}

/* 左侧分类导航 */
.cats {
  flex: none;
  width: 196px;
  display: flex;
  flex-direction: column;
  gap: 4px;
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

/* 右侧内容面板 */
.panel {
  flex: 1;
  min-width: 0;
  padding: 0 4px;
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

.rows {
  margin: 4px 0 0;
}

.row {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 13px 0;
  border-top: 1px solid var(--line-soft);
}

.row:first-child {
  border-top: none;
}

dt {
  flex: none;
  width: 88px;
  font-size: 12.5px;
  color: var(--muted);
}

dd {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  margin: 0;
  font-size: 13px;
}

dd.grow {
  flex-direction: column;
  align-items: stretch;
  gap: 6px;
}

.ops {
  flex: none;
  display: flex;
  gap: 8px;
}

.path {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.strong {
  font-weight: 600;
}

.muted {
  font-size: 12px;
  color: var(--faint);
}

.warn {
  font-weight: 600;
  color: var(--warn);
}

.hint {
  font-size: 11.5px;
  color: var(--faint);
}

.hint.wrap {
  white-space: normal;
}

.vip {
  padding: 1px 7px;
  font-size: 11px;
  color: var(--accent-dark);
  background: var(--accent-soft);
  border-radius: 999px;
}

select,
input:not([type="checkbox"]) {
  padding: 7px 10px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  transition: border-color 0.15s ease;
}

select:hover,
input:hover {
  border-color: var(--accent-line);
}

select:focus,
input:focus {
  outline: none;
  border-color: var(--accent-line);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

input::placeholder {
  color: var(--faint);
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

.ghost {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
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
  opacity: 0.5;
  cursor: not-allowed;
}

.ghost.danger:hover {
  color: var(--err);
  border-color: var(--fail-line);
  background: var(--fail-bg);
}

.loading {
  margin: 22px 0 0;
  font-size: 12.5px;
  color: var(--faint);
}
</style>
