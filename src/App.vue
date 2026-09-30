<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { animate, stagger } from "animejs";

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

/** 切页编排：新页可见卡的直接子元素（标题/工具条/列表）依次浮起。
    根元素仍走 .page-in 淡入，两者不冲突（不同元素）。
    只动 transform/opacity，零视觉变化；减少动态下不演。 */
watch(page, async () => {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  await nextTick();
  requestAnimationFrame(() => {
    const card = [...document.querySelectorAll(".content .card")].find((c) => c.offsetParent !== null);
    if (!card) return;
    const kids = [...card.children].filter((el) => getComputedStyle(el).position !== "fixed").slice(0, 10);
    if (!kids.length) return;
    animate(kids, {
      opacity: [0, 1],
      translateY: [10, 0],
      duration: 460,
      delay: stagger(45),
      ease: "outExpo",
    });
  });
});
const login = ref({ logged_in: false, uname: "", face: "", mid: 0, vip: false, vip_label: "" });
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
  // 首帧之后再开颜色过渡：启动时要的是"立刻正确"，不是"渐变色"
  requestAnimationFrame(() => document.documentElement.classList.add("ready"));
  applyUiScale();
  window.addEventListener("resize", applyUiScale);
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
  // 自动更新：设置里开了「自动检测」就启动时静默查一次（只提示，不自动装）
  if (settings.value?.update_check) {
    api
      .updateCheck()
      .then((update) => {
        if (update) showToast(`发现新版本 v${update.version}，到「设置 · 应用更新」下载安装`);
      })
      .catch(() => {});
  }
  // 上次没下完的任务：设置里开了「启动时自动继续」就接着下（分片记录让已下载的字节不重下）
  if (settings.value?.resume_on_start) {
    api
      .resumePending()
      .then((count) => {
        if (count > 0) showToast(`继续未完成的下载：${count} 个任务`);
      })
      .catch(() => {});
  }

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
    api.setWindowBackground(mode === "dark" ? "#0f1011" : "#f5f3f4");
  }
}

/** 整体 UI 随窗口等比缩放：以默认窗口 1100×740 为 1.0，取宽高比较小的一边
 *  （避免只拉一边时另一边溢出），夹在 0.85–1.3，并按 0.05 量化——
 *  拖拽调节大小时不会逐帧跳变。默认尺寸下恰好 1.0，仪器断言不受影响。 */
function applyUiScale() {
  const raw = Math.min(window.innerWidth / 1100, window.innerHeight / 740);
  const zoom = Math.round(Math.min(1.3, Math.max(0.85, raw)) * 20) / 20;
  document.documentElement.style.zoom = String(zoom);
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
/** 内容库点「去解析」时带过来的来源：设置后切到解析页，那边看到就自动解析 */
const pendingSource = ref(null);

function openSource(url) {
  pendingSource.value = { url, stamp: Date.now() };
  page.value = "parse";
}

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

/** 主题由标题栏的菜单选定；切换即时保存 */
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
  naming_template: "{title}.{ext}",
  naming_presets: [],
  rename_conflict: "skip",
  container: "mp4",
  codec_pref: "auto",
  quality_fallback: "nearest",
  download_cover: false,
  audio_format: "source",
  image_format: "source",
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
      @set-theme="setTheme"
    />

    <div class="body">
      <Sidebar
        :current="page"
        :login="login"
        :queue="queue"
        @navigate="page = $event"
      />

      <main class="content">
        <!-- 解析页用 v-show 常驻：切到别的栏目再回来，已解析的清单与勾选都还在。
             其余页面按需挂载，所以这里不用 v-else-if 链。 -->
        <ParsePage
          v-show="page === 'parse'"
          :class="{ 'page-in': page === 'parse' }"
          :login="login"
          :settings="settings"
          @toast="showToast"
          @goto="page = $event"
          :pending-source="pendingSource"
        />
        <TransferPage
          v-if="page === 'transfer'"
          :class="{ 'page-in': page === 'transfer' }"
          :tasks="tasks"
          @cancel="cancelTask"
          @open="openPath"
          @clear="clearFinished"
        />
        <SettingsPage
          v-if="page === 'settings'"
          :class="{ 'page-in': page === 'settings' }"
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
        <AboutPage v-if="page === 'about'" class="page-in" :version="version" />
        <LibraryPage
          v-if="page === 'library'"
          class="page-in"
          :login="login"
          @goto="page = $event"
          @open-source="openSource"
          @login="showLogin = true"
        />
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

.toast-enter-active {
  transition: opacity 0.26s var(--ease-out-expo), transform 0.26s var(--ease-out-expo);
}

.toast-leave-active {
  transition: opacity 0.16s ease, transform 0.16s ease;
}

.toast-enter-from {
  opacity: 0;
  transform: translate(-50%, 14px) scale(0.96);
}

.toast-leave-to {
  opacity: 0;
  transform: translate(-50%, 6px) scale(0.98);
}
</style>
