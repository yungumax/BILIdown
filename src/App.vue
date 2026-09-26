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
const resolvedTheme = ref("light");

const systemPrefersDark =
  typeof window !== "undefined" &&
  window.matchMedia("(prefers-color-scheme: dark)");

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
  systemPrefersDark?.addEventListener("change", () => {
    if (settings.value?.theme === "system") applyTheme("system");
  });

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

/** 把主题写到根元素上，CSS 令牌据此切换 */
function applyTheme(choice) {
  const resolved =
    choice === "system" ? (systemPrefersDark?.matches ? "dark" : "light") : choice;
  resolvedTheme.value = resolved === "dark" ? "dark" : "light";
  document.documentElement.dataset.theme = resolvedTheme.value;
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
async function changeSettings(patch) {
  if (!settings.value) return;
  const next = { ...settings.value, ...patch };
  settings.value = next;
  applyTheme(next.theme);
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

function toggleTheme() {
  changeSettings({ theme: resolvedTheme.value === "dark" ? "light" : "dark" });
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
      :theme="resolvedTheme"
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
          :resolved-theme="resolvedTheme"
          @toast="showToast"
          @login="openLogin"
          @logout="doLogout"
          @change="changeSettings"
          @reload="loadSettings"
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
