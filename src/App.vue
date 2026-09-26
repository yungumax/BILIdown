<script setup>
import { computed, onMounted, onUnmounted, ref } from "vue";

import * as api from "./api";
import TitleBar from "./components/TitleBar.vue";
import Sidebar from "./components/Sidebar.vue";
import LoginDialog from "./components/LoginDialog.vue";
import ParsePage from "./pages/ParsePage.vue";
import TransferPage from "./pages/TransferPage.vue";
import SettingsPage from "./pages/SettingsPage.vue";
import AboutPage from "./pages/AboutPage.vue";
import LibraryPage from "./pages/LibraryPage.vue";

const RUNNING = ["queued", "downloading", "merging"];

const page = ref("parse");
const login = ref({ logged_in: false, uname: "", mid: 0, vip: false, vip_label: "" });
const version = ref("");
const outputDir = ref("");
const tasks = ref([]);
const toastText = ref("");
const showLogin = ref(false);
const qr = ref(null);
const loginState = ref("loading");
const settings = ref(null);
const settingsEnv = ref(null);


let unlistenTask = null;
let pollTimer = null;
let toastTimer = null;

const queue = computed(() => {
  const running = tasks.value.filter((task) => RUNNING.includes(task.status));
  return {
    active: running.length,
    speed: running.reduce((sum, task) => sum + (task.speed_bps || 0), 0),
  };
});

onMounted(async () => {
  try {
    const status = await api.appStatus();
    login.value = status.login;
    outputDir.value = status.output_dir;
    version.value = status.version;
  } catch (error) {
    showToast(String(error));
  }

  await loadSettings();
  refreshFfmpeg();

  unlistenTask = await api.onTaskUpdate((task) => {
    const index = tasks.value.findIndex((item) => item.id === task.id);
    if (index === -1) tasks.value.unshift(task);
    else tasks.value[index] = task;
  });
});

onUnmounted(() => {
  if (unlistenTask) unlistenTask();
  stopPolling();
  clearTimeout(toastTimer);
});

/**
 * 把主题模式写到根元素上：light / dark 直接生效，
 * system 交给 CSS 的 prefers-color-scheme media query。
 * 注意：启动首帧不依赖这里——那是 Rust 初始化脚本的职责，否则会先看到一次配色翻转。
 * 切换瞬时完成；同步窗口原生底色，避免边缘/滚动条区域露出旧色。
 */
function applyTheme(mode) {
  document.documentElement.dataset.theme = mode;
  // system 模式的原生底色由 Rust 启动时按真实系统主题设置，前端不干预；
  // 显式切换时同步，避免边缘露出旧色
  if (mode !== "system") {
    api.setWindowBackground(mode === "dark" ? "#1b1d21" : "#f5f3f4");
  }
}

/** ffmpeg 状态单独取：探测要起子进程（约 0.8 秒），不能拖慢设置读取与主题生效 */
async function refreshFfmpeg(refresh = false) {
  try {
    const status = await api.ffmpegStatus(refresh);
    if (settingsEnv.value) {
      settingsEnv.value = {
        ...settingsEnv.value,
        ffmpeg_ok: status.ok,
        ffmpeg_info: status.info,
      };
    }
  } catch {
    // 状态展示失败不影响使用
  }
}

async function loadSettings() {
  try {
    const data = await api.appSettings();
    settingsEnv.value = data;
    settings.value = data.settings;
    applyTheme(data.settings.theme);
  } catch (error) {
    showToast(String(error));
  }
}

/** 改动即时保存；失败则回读一次，避免界面与磁盘不一致 */
/** 设置页点「保存」：整份提交，成功后同步主题 */
async function saveSettings(next) {
  const ffmpegChanged = next.ffmpeg_path !== settings.value?.ffmpeg_path;
  try {
    const data = await api.updateSettings(next);
    settingsEnv.value = data;
    settings.value = data.settings;
    applyTheme(data.settings.theme);
    showToast("设置已保存");
    // 换了 ffmpeg 才需要重新探测，避免每次保存都起子进程
    if (ffmpegChanged) refreshFfmpeg(true);
  } catch (error) {
    showToast(String(error));
    await loadSettings();
  }
}

/** 右上角按钮：浅色 → 深色 → 跟随系统 循环，切换即时保存 */
function toggleTheme() {
  const order = ["light", "dark", "system"];
  const current = settings.value?.theme || "system";
  const next = order[(order.indexOf(current) + 1) % order.length];
  setTheme(next);
}

async function setTheme(theme) {
  if (!settings.value) return;
  const next = { ...settings.value, theme };
  try {
    const data = await api.updateSettings(next);
    settingsEnv.value = data;
    settings.value = data.settings;
    applyTheme(data.settings.theme);
  } catch (error) {
    showToast(String(error));
    await loadSettings();
  }
}

const DEFAULT_SETTINGS = {
  max_concurrent_tasks: 2,
  chunk_concurrency: 4,
  chunk_mb: 4,
  keep_temp: false,
  naming_template: "{title}",
  naming_presets: [],
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
  proxy: "",
  theme: "system",
};

async function resetSettings() {
  if (!settings.value) return;
  await saveSettings({ ...DEFAULT_SETTINGS, output_dir: settings.value.output_dir });
  showToast("已恢复默认设置（保存位置不变）");
}

function showToast(text) {
  if (!text) return;
  toastText.value = text;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toastText.value = ""), 3600);
}

async function cancelTask(taskId) {
  try {
    await api.cancelDownload(taskId);
  } catch (error) {
    showToast(String(error));
  }
}

async function openPath(path) {
  try {
    await api.openPath(path);
  } catch (error) {
    showToast(String(error));
  }
}

function clearFinished() {
  tasks.value = tasks.value.filter((task) => RUNNING.includes(task.status));
}

function openLogin() {
  showLogin.value = true;
  if (!login.value.logged_in) {
    loginState.value = "loading";
    refreshQr();
  }
}

async function refreshQr() {
  stopPolling();
  qr.value = null;
  loginState.value = "loading";
  try {
    qr.value = await api.loginQrcode();
    loginState.value = "pending";
    startPolling();
  } catch (error) {
    loginState.value = "error";
    showToast(String(error));
  }
}

function startPolling() {
  stopPolling();
  pollTimer = setInterval(async () => {
    if (!qr.value) return;
    try {
      const result = await api.loginPoll(qr.value.qrcode_key);
      loginState.value = result.state;
      if (result.state === "confirmed") {
        login.value = result.login;
        stopPolling();
        showToast(`已登录：${result.login.uname}`);
        setTimeout(() => (showLogin.value = false), 1100);
      } else if (result.state === "expired") {
        stopPolling();
      }
    } catch (error) {
      loginState.value = "error";
      showToast(String(error));
      stopPolling();
    }
  }, 2000);
}

function stopPolling() {
  if (pollTimer) clearInterval(pollTimer);
  pollTimer = null;
}

function closeLogin() {
  showLogin.value = false;
  stopPolling();
}

async function doLogout() {
  try {
    login.value = await api.logout();
    showLogin.value = false;
    showToast("已退出登录");
  } catch (error) {
    showToast(String(error));
  }
}
</script>

<template>
  <div class="app">
    <TitleBar
      :login="login"
      :version="version"
      :theme="settings?.theme || 'system'"
      @login="openLogin"
      @toggle-theme="toggleTheme"
    />

    <div class="body">
      <Sidebar
        :current="page"
        :login="login"
        :queue="queue"
        @navigate="page = $event"
      />

      <main class="content">
        <ParsePage
          v-if="page === 'parse'"
          :login="login"
          :settings="settings"
          @toast="showToast"
          @goto="page = $event"
        />
        <TransferPage
          v-else-if="page === 'transfer'"
          :tasks="tasks"
          @cancel="cancelTask"
          @open="openPath"
          @clear="clearFinished"
        />
        <SettingsPage
          v-else-if="page === 'settings'"
          :login="login"
          :settings="settings"
          :env="settingsEnv"
          @toast="showToast"
          @login="openLogin"
          @logout="doLogout"
          @save="saveSettings"
          @reload="loadSettings"
          @reset="resetSettings"
        />
        <AboutPage v-else-if="page === 'about'" :version="version" />
        <LibraryPage v-else @goto="page = $event" />
      </main>
    </div>

    <Transition name="toast">
      <div v-if="toastText" class="toast">{{ toastText }}</div>
    </Transition>

    <LoginDialog
      v-if="showLogin"
      :qr="qr"
      :state="loginState"
      :login="login"
      @close="closeLogin"
      @refresh="refreshQr"
      @logout="doLogout"
    />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.body {
  flex: 1;
  min-height: 0;
  display: flex;
}

.content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: 18px 22px 22px;
}

.toast {
  position: fixed;
  left: 50%;
  bottom: 26px;
  transform: translateX(-50%);
  padding: 9px 18px;
  font-size: 12.5px;
  color: #fff;
  background: var(--text);
  color: var(--card);
  border-radius: 999px;
  box-shadow: 0 8px 24px rgba(60, 40, 50, 0.2);
  z-index: 30;
}

.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translate(-50%, 8px);
}
</style>
