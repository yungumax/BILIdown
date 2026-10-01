<script setup>
import { computed, onMounted, ref, watch } from "vue";
import QRCode from "qrcode";
import * as api from "../api";

const props = defineProps({
  qr: { type: Object, default: null },
  state: { type: String, default: "loading" },
  login: { type: Object, default: () => ({ logged_in: false }) },
});
const emit = defineEmits(["close", "refresh", "logout", "web-confirmed"]);

const canvas = ref(null);

const STATE_TEXT = {
  loading: "正在申请二维码…",
  pending: "打开 B 站 App，点「我的 → 扫一扫」扫描上方二维码",
  scanned: "已扫码，请在手机上点确认",
  confirmed: "登录成功",
  expired: "二维码已过期，请重新生成",
  error: "二维码获取失败",
};

const hint = computed(() => STATE_TEXT[props.state] || "");
const showQr = computed(
  () => props.qr && ["pending", "scanned", "confirmed"].includes(props.state)
);

async function draw() {
  if (!props.qr || !canvas.value) return;
  await QRCode.toCanvas(canvas.value, props.qr.url, {
    width: 216,
    margin: 1,
    errorCorrectionLevel: "M",
    color: { dark: "#26262b", light: "#ffffff" },
  });
}

onMounted(draw);
watch(() => props.qr, draw);

/* ── 网页登录（内嵌 B 站官方登录页）────────────────────────
   账密/短信/扫码都走 B 站自己的页面，应用只负责开窗与收 Cookie */
const mode = ref("qr");
const webBusy = ref(false);

async function openWebLogin() {
  if (webBusy.value) return;
  webBusy.value = true;
  try {
    await api.webLoginOpen();
    // 开窗后轮询 Cookie：登录成功（三件套齐）即收割并通知主应用
    for (let i = 0; i < 150; i++) {
      await new Promise((r) => setTimeout(r, 2000));
      try {
        const login = await api.webLoginCookies();
        // 成功：让主应用刷新登录态并关弹窗
        mode.value = "qr";
        emit("web-confirmed", login);
        return;
      } catch {
        // 未登录或窗口已关——继续轮询直到用户放弃（关闭弹窗时组件卸载）
      }
    }
  } finally {
    webBusy.value = false;
  }
}

async function closeWebLogin() {
  mode.value = "qr";
  try {
    await api.webLoginClose();
  } catch {}
}
</script>

<template>
  <div class="backdrop" @click.self="emit('close')">
    <div class="dialog pop-in" role="dialog" aria-modal="true">
      <header class="head">
        <h2>{{ login.logged_in ? "账号" : mode === "qr" ? "扫码登录" : "网页登录" }}</h2>
        <button class="close" title="关闭" @click="emit('close')">✕</button>
      </header>

      <template v-if="login.logged_in">
        <div class="account">
          <p class="uname">{{ login.uname }}</p>
          <p class="sub">
            <span class="num">UID {{ login.mid }}</span>
            <span v-if="login.vip" class="vip">{{ login.vip_label || "大会员" }}</span>
          </p>
        </div>
        <p class="tip">
          已登录状态下可下载 1080P 及以上清晰度；大会员可解锁 4K、HDR、杜比视界与无损音轨。
        </p>
        <div class="actions">
          <button class="ghost danger" @click="emit('logout')">退出登录</button>
          <button class="primary" @click="emit('close')">完成</button>
        </div>
      </template>

      <template v-else>
        <!-- 登录方式切换 -->
        <div class="login-tabs">
          <button :class="{ active: mode === 'qr' }" @click="mode = 'qr'">扫码登录</button>
          <button
            :class="{ active: mode === 'web' }"
            @click="if (mode !== 'web') { mode = 'web'; openWebLogin(); }"
          >
            {{ webBusy ? "等待登录…" : "网页登录" }}
          </button>
        </div>

        <template v-if="mode === 'web'">
          <div class="web-login-hint">
            <p class="hint">已打开 B 站官方登录窗口（独立小窗）。</p>
            <p class="hint">账号密码、短信验证、扫码均可——全部在 B 站官方页面完成，本应用不接触你的密码。</p>
            <p class="hint">登录成功后点下方按钮完成绑定。</p>
          </div>
          <div class="actions">
            <button class="ghost" @click="closeWebLogin">返回扫码</button>
            <span class="spacer"></span>
            <button class="primary" :disabled="webBusy" @click="openWebLogin">
              {{ webBusy ? "检测登录中…" : "我已登录，完成绑定" }}
            </button>
          </div>
        </template>

        <template v-else>
        <div class="qr-area">
          <canvas v-show="showQr" ref="canvas"></canvas>
          <div v-if="state === 'loading'" class="spinner" aria-hidden="true"></div>
          <div v-else-if="state === 'expired' || state === 'error'" class="retry">
            <button class="ghost" @click="emit('refresh')">重新生成二维码</button>
          </div>
        </div>
        <p class="hint" :class="{ ok: state === 'confirmed' }">{{ hint }}</p>
        <p v-if="qr?.url" class="link-hint">
          终端或手机无法扫描时，可在已登录 B 站的浏览器里打开同一链接完成授权。
        </p>
        </template>
      </template>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  background: var(--shade);
  z-index: 20;
}

.dialog {
  width: 336px;
  padding: 20px;
  background: var(--card);
  border-radius: 16px;
  box-shadow: 0 18px 48px rgba(60, 40, 50, 0.18);
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
}

h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
}

.close {
  width: 30px;
  height: 30px;
  font-size: 12px;
  color: var(--faint);
  border-radius: var(--radius-sm);
}

.close:hover {
  background: var(--raised);
  color: var(--text);
}

.qr-area {
  display: grid;
  place-items: center;
  min-height: 216px;
  padding: 6px;
  background: #fff;
  border: 1px solid var(--line);
  border-radius: var(--radius);
}

.qr-area canvas {
  display: block;
}

.spinner {
  width: 30px;
  height: 30px;
  border: 2px solid var(--line);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.9s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.hint {
  margin: 13px 0 0;
  font-size: 12.5px;
  line-height: 1.6;
  text-align: center;
  color: var(--muted);
}

.hint.ok {
  color: var(--ok);
  font-weight: 600;
}

.link-hint {
  margin: 9px 0 0;
  font-size: 11.5px;
  line-height: 1.6;
  text-align: center;
  color: var(--faint);
}

.account {
  padding: 16px;
  text-align: center;
  background: var(--raised);
  border-radius: var(--radius);
}

.uname {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
}

.sub {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  margin: 6px 0 0;
  font-size: 12px;
  color: var(--muted);
}

.vip {
  padding: 1px 8px;
  font-size: 11px;
  color: var(--accent-dark);
  background: var(--accent-soft);
  border-radius: 999px;
}

.tip {
  margin: 12px 0 0;
  font-size: 11.5px;
  line-height: 1.65;
  color: var(--faint);
  text-align: center;
}

.actions {
  display: flex;
  gap: 10px;
  margin-top: 16px;
}

.actions button {
  flex: 1;
}

.primary {
  padding: 9px;
  font-weight: 600;
  color: #fff;
  background: var(--accent);
  border-radius: var(--radius-sm);
}

.primary:hover {
  background: var(--accent-dark);
}

.ghost {
  padding: 9px 14px;
  font-size: 12.5px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.ghost:hover {
  border-color: var(--line);
  background: var(--raised);
}

.ghost.danger:hover {
  color: var(--err);
  border-color: var(--fail-line);
  background: var(--fail-bg);
}
</style>

<style scoped>
/* 第六轮追加：登录成功提示强调弹出 */
.hint.ok {
  animation: hint-ok 320ms var(--ease-out-expo);
}

@keyframes hint-ok {
  0% { transform: translateY(4px); opacity: 0.3; }
  60% { transform: none; opacity: 1; }
  100% { transform: none; opacity: 1; }
}
</style>

<style scoped>
/* 登录方式切换 tabs（与解析页 tabs 同语言） */
.login-tabs {
  display: flex;
  gap: 8px;
  margin: 0 0 12px;
}

.login-tabs button {
  padding: 5px 14px;
  font: inherit;
  font-size: 12.5px;
  color: var(--muted);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background var(--motion-fast) var(--ease-out), color var(--motion-fast) var(--ease-out), transform var(--motion-fast) var(--ease-out);
}

.login-tabs button:not(.active):hover {
  color: var(--text);
  transform: translateY(-1px);
}

.login-tabs button.active {
  color: #fff;
  background: var(--accent);
  border-color: var(--accent);
}

.web-login-hint .hint {
  margin: 6px 0 0;
  line-height: 1.6;
}

/* tab 切换动效：内容块淡入上浮 */
.dialog > .login-tabs + .web-login-hint,
.dialog > template + .qr-area {
  animation: mode-in 260ms var(--ease-out);
}

@keyframes mode-in {
  from { opacity: 0; transform: translateY(6px); }
  to { opacity: 1; transform: none; }
}

/* 网页登录等待时主钮不扫光只呼吸（轻提示） */
.primary:disabled {
  opacity: 0.6;
}
</style>
