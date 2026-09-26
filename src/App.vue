<script setup>
import { onMounted, onUnmounted, ref } from "vue";
import * as api from "./api";
import PreviewCard from "./components/PreviewCard.vue";
import TaskList from "./components/TaskList.vue";
import LoginDialog from "./components/LoginDialog.vue";

const input = ref("");
const probing = ref(false);
const errorText = ref("");
const probe = ref(null);
const tasks = ref([]);
const login = ref({ logged_in: false, uname: "", mid: 0, vip: false, vip_label: "" });
const outputDir = ref("");
const version = ref("");

const showLogin = ref(false);
const qr = ref(null);
const loginState = ref("loading");

let unlistenTask = null;
let pollTimer = null;

onMounted(async () => {
  try {
    const status = await api.appStatus();
    login.value = status.login;
    outputDir.value = status.output_dir;
    version.value = status.version;
  } catch (e) {
    errorText.value = String(e);
  }

  unlistenTask = await api.onTaskUpdate((task) => {
    const index = tasks.value.findIndex((item) => item.id === task.id);
    if (index === -1) {
      tasks.value.unshift(task);
    } else {
      tasks.value[index] = task;
    }
  });
});

onUnmounted(() => {
  if (unlistenTask) unlistenTask();
  stopPolling();
});

async function runProbe() {
  const value = input.value.trim();
  if (!value || probing.value) return;
  probing.value = true;
  errorText.value = "";
  try {
    probe.value = await api.probeVideo(value);
  } catch (e) {
    probe.value = null;
    errorText.value = String(e);
  } finally {
    probing.value = false;
  }
}

async function startDownload({ quality, audio }) {
  if (!probe.value) return;
  errorText.value = "";
  try {
    await api.startDownload({
      bvid: probe.value.bvid,
      cid: probe.value.cid,
      title: probe.value.title,
      quality,
      audio,
      cover: probe.value.cover,
    });
    probe.value = null;
    input.value = "";
  } catch (e) {
    errorText.value = String(e);
  }
}

async function cancelTask(taskId) {
  try {
    await api.cancelDownload(taskId);
  } catch (e) {
    errorText.value = String(e);
  }
}

async function openPath(path) {
  try {
    await api.openPath(path);
  } catch (e) {
    errorText.value = String(e);
  }
}

async function pickOutputDir() {
  try {
    outputDir.value = await api.chooseOutputDir();
  } catch (e) {
    errorText.value = String(e);
  }
}

function clearFinished() {
  tasks.value = tasks.value.filter((t) =>
    ["queued", "downloading", "merging"].includes(t.status)
  );
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
  } catch (e) {
    loginState.value = "error";
    errorText.value = String(e);
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
        setTimeout(() => (showLogin.value = false), 1000);
      } else if (result.state === "expired") {
        stopPolling();
      }
    } catch (e) {
      loginState.value = "error";
      errorText.value = String(e);
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
  } catch (e) {
    errorText.value = String(e);
  }
}
</script>

<template>
  <div class="app">
    <header class="topbar">
      <div class="brand">
        <svg class="mark" viewBox="0 0 24 24" aria-hidden="true">
          <rect width="24" height="24" rx="6" fill="#fb7299" />
          <path
            d="M12 5.8v6.6"
            stroke="#fff"
            stroke-width="2.1"
            stroke-linecap="round"
          />
          <path
            d="M8.5 10.2 12 13.7l3.5-3.5"
            fill="none"
            stroke="#fff"
            stroke-width="2.1"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
          <path d="M7.6 17.4h8.8" stroke="#fff" stroke-width="2.1" stroke-linecap="round" />
        </svg>
        <span class="name">BILIdown</span>
      </div>

      <button
        class="login-chip"
        :class="{ on: login.logged_in }"
        @click="openLogin"
      >
        <template v-if="login.logged_in">
          <span class="dot"></span>
          <span>{{ login.uname }}</span>
          <span v-if="login.vip" class="vip">{{ login.vip_label || "大会员" }}</span>
        </template>
        <template v-else>
          <span class="dot off"></span>
          <span>未登录 · 点此扫码</span>
        </template>
      </button>
    </header>

    <section class="intake">
      <form class="intake-form" @submit.prevent="runProbe">
        <input
          v-model="input"
          :disabled="probing"
          placeholder="粘贴视频链接、BV 号或 av 号"
          spellcheck="false"
          autocomplete="off"
        />
        <button class="primary" type="submit" :disabled="probing || !input.trim()">
          {{ probing ? "解析中" : "解析" }}
        </button>
      </form>
      <p v-if="errorText" class="error-line">{{ errorText }}</p>
    </section>

    <main class="body">
      <PreviewCard
        v-if="probe"
        :probe="probe"
        @start="startDownload"
        @dismiss="probe = null"
      />
      <TaskList
        :tasks="tasks"
        @cancel="cancelTask"
        @open="openPath"
        @clear="clearFinished"
      />
    </main>

    <footer class="statusbar">
      <button class="path" :title="outputDir" @click="pickOutputDir">
        保存到 {{ outputDir }}
      </button>
      <span class="spacer"></span>
      <button class="ghost" @click="openPath(outputDir)">打开目录</button>
      <span class="version num">v{{ version }}</span>
    </footer>

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

.topbar {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 13px 20px;
  border-bottom: 1px solid var(--line-soft);
}

.brand {
  display: flex;
  align-items: center;
  gap: 9px;
}

.mark {
  width: 22px;
  height: 22px;
}

.name {
  font-size: 14.5px;
  font-weight: 600;
  letter-spacing: 0.2px;
}

.login-chip {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 6px 13px;
  font-size: 12.5px;
  color: var(--muted);
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 999px;
  transition: border-color 0.15s ease, color 0.15s ease;
}

.login-chip:hover {
  color: var(--text);
  border-color: #3a4150;
}

.login-chip.on {
  color: var(--text);
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--ok);
}

.dot.off {
  background: var(--faint);
}

.vip {
  padding: 1px 7px;
  font-size: 11px;
  color: var(--accent);
  background: var(--accent-soft);
  border-radius: 999px;
}

.intake {
  flex: none;
  padding: 16px 20px 6px;
}

.intake-form {
  display: flex;
  gap: 10px;
}

.intake-form input {
  flex: 1;
  padding: 11px 14px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  transition: border-color 0.15s ease;
}

.intake-form input::placeholder {
  color: var(--faint);
}

.intake-form input:focus {
  outline: none;
  border-color: var(--accent);
}

.intake-form input:disabled {
  opacity: 0.6;
}

.primary {
  flex: none;
  padding: 11px 24px;
  font-weight: 600;
  color: #240d16;
  background: var(--accent);
  border-radius: var(--r-md);
}

.primary:hover:not(:disabled) {
  background: #ff86a8;
}

.primary:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.error-line {
  margin: 10px 2px 0;
  font-size: 12.5px;
  color: var(--err);
  line-height: 1.5;
}

.body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 12px 20px 16px;
}

.statusbar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 9px 20px;
  font-size: 12px;
  color: var(--faint);
  border-top: 1px solid var(--line-soft);
}

.path {
  max-width: 58%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--muted);
  padding: 3px 7px;
  border-radius: var(--r-sm);
}

.path:hover {
  color: var(--text);
  background: var(--hover);
}

.spacer {
  flex: 1;
}

.ghost {
  padding: 3px 9px;
  color: var(--muted);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
}

.ghost:hover {
  color: var(--text);
  border-color: #3a4150;
  background: var(--hover);
}

.version {
  color: var(--faint);
}
</style>
