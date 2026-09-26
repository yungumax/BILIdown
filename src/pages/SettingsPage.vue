<script setup>
import { onMounted, ref } from "vue";
import * as api from "../api";

const props = defineProps({
  login: { type: Object, required: true },
  outputDir: { type: String, default: "" },
});
const emit = defineEmits(["toast", "login", "logout", "output-dir"]);

const settings = ref(null);
const loading = ref(true);

onMounted(load);

async function load() {
  loading.value = true;
  try {
    settings.value = await api.appSettings();
  } catch (error) {
    emit("toast", String(error));
  } finally {
    loading.value = false;
  }
}

async function chooseDir() {
  try {
    emit("output-dir", await api.chooseOutputDir());
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

function parentDir(path) {
  const index = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return index > 0 ? path.slice(0, index) : path;
}
</script>

<template>
  <div>
    <section class="card">
      <h1>设置</h1>
      <p class="lead">下载位置、账号与运行环境。</p>

      <dl class="rows">
        <div class="row">
          <dt>保存位置</dt>
          <dd>
            <span class="path" :title="outputDir">{{ outputDir || "—" }}</span>
          </dd>
          <div class="ops">
            <button class="ghost" @click="chooseDir">选择目录</button>
            <button class="ghost" @click="open(outputDir)">打开</button>
          </div>
        </div>

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
            <span class="path" :title="settings?.cookies_path">
              {{ settings?.cookies_path || "—" }}
            </span>
            <span class="muted">
              {{ settings?.cookies_saved ? "已保存，等同账号密码，请勿分享" : "尚未保存" }}
            </span>
          </dd>
          <div class="ops">
            <button
              class="ghost"
              :disabled="!settings?.cookies_path"
              @click="open(parentDir(settings.cookies_path))"
            >
              打开目录
            </button>
          </div>
        </div>

        <div class="row">
          <dt>ffmpeg</dt>
          <dd>
            <span :class="settings?.ffmpeg_ok ? 'strong' : 'warn'">
              {{ settings?.ffmpeg_ok ? "已就绪" : "不可用" }}
            </span>
            <span class="muted wrap">{{ settings?.ffmpeg_info || (loading ? "检测中…" : "—") }}</span>
          </dd>
          <div class="ops">
            <button class="ghost" @click="load">重新检测</button>
          </div>
        </div>

        <div class="row">
          <dt>版本</dt>
          <dd>
            <span class="num">BILIdown v{{ settings?.version || "—" }}</span>
            <span class="muted">Tauri 2 · Rust · Vue 3</span>
          </dd>
          <div class="ops"></div>
        </div>
      </dl>
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

.rows {
  margin: 20px 0 0;
}

.row {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 14px 0;
  border-top: 1px solid var(--line-soft);
}

dt {
  flex: none;
  width: 86px;
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

.muted.wrap {
  white-space: normal;
}

.warn {
  font-weight: 600;
  color: var(--warn);
}

.vip {
  padding: 1px 7px;
  font-size: 11px;
  color: var(--accent-dark);
  background: var(--accent-soft);
  border-radius: 999px;
}

.ghost {
  padding: 6px 12px;
  font-size: 12.5px;
  color: var(--text);
  background: #fff;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.ghost:hover:not(:disabled) {
  border-color: #ded6da;
  background: var(--raised);
}

.ghost:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.ghost.danger:hover {
  color: var(--err);
  border-color: #f0cfcc;
  background: #fdf6f5;
}
</style>
