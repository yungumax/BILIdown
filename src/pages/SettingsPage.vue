<script setup>
import { computed, ref } from "vue";
import * as api from "../api";

const props = defineProps({
  login: { type: Object, required: true },
  settings: { type: Object, default: null },
  env: { type: Object, default: null },
  resolvedTheme: { type: String, default: "light" },
});
const emit = defineEmits(["toast", "login", "logout", "change", "reload"]);

const THEMES = [
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
  { value: "system", label: "跟随系统" },
];

const QUALITIES = [
  { value: 0, label: "自动（该视频可用的最高档）" },
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
  { value: "normal", label: "普通音轨" },
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

/** 代理输入框：未编辑时显示已保存值 */
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
      <h1>设置</h1>
      <p class="lead">下载偏好、外观与运行环境。改动即时保存。</p>

      <template v-if="settings">
        <h2>下载</h2>
        <dl class="rows">
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
              <select :value="settings.chunk_mb" @change="patchNumber('chunk_mb', $event.target.value)">
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

        <h2>下载偏好</h2>
        <dl class="rows">
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
              <span class="hint">解析后预选这一档，仍可逐条修改</span>
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
        </dl>

        <h2>网络</h2>
        <dl class="rows">
          <div class="row">
            <dt>代理</dt>
            <dd class="grow">
              <input
                v-model="proxyValue"
                spellcheck="false"
                placeholder="http://127.0.0.1:7890（留空为直连）"
                @keydown.enter="applyProxy"
              />
              <span class="hint">改完按回车或点「应用」生效</span>
            </dd>
            <div class="ops">
              <button class="ghost" @click="applyProxy">应用</button>
            </div>
          </div>
        </dl>

        <h2>外观</h2>
        <dl class="rows">
          <div class="row">
            <dt>主题</dt>
            <dd>
              <div class="segmented">
                <button
                  v-for="item in THEMES"
                  :key="item.value"
                  :class="{ active: settings.theme === item.value }"
                  @click="patch('theme', item.value)"
                >
                  {{ item.label }}
                </button>
              </div>
              <span class="hint">
                当前显示：{{ resolvedTheme === "dark" ? "深色" : "浅色" }}
              </span>
            </dd>
          </div>
        </dl>

        <h2>账号</h2>
        <dl class="rows">
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
        </dl>

        <h2>运行环境</h2>
        <dl class="rows">
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
      </template>

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

h2 {
  margin: 24px 0 0;
  font-size: 13px;
  font-weight: 700;
  color: var(--muted);
}

.rows {
  margin: 8px 0 0;
}

.row {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 13px 0;
  border-top: 1px solid var(--line-soft);
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

.segmented {
  display: inline-flex;
  padding: 2px;
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.segmented button {
  padding: 5px 14px;
  font-size: 12.5px;
  color: var(--muted);
  border-radius: 6px;
  transition: all 0.15s ease;
}

.segmented button:hover {
  color: var(--text);
}

.segmented button.active {
  color: #fff;
  background: var(--accent);
  font-weight: 600;
}

.ghost {
  padding: 6px 12px;
  font-size: 12.5px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
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
