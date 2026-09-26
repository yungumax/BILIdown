<script setup>
import { computed, onMounted, ref, watch } from "vue";
import QRCode from "qrcode";

const props = defineProps({
  qr: { type: Object, default: null },
  state: { type: String, default: "loading" },
  login: { type: Object, default: () => ({ logged_in: false }) },
});
const emit = defineEmits(["close", "refresh", "logout"]);

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
</script>

<template>
  <div class="backdrop" @click.self="emit('close')">
    <div class="dialog" role="dialog" aria-modal="true">
      <header class="head">
        <h2>{{ login.logged_in ? "账号" : "扫码登录" }}</h2>
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
  width: 26px;
  height: 26px;
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
  width: 26px;
  height: 26px;
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
  border-color: #ded6da;
  background: var(--raised);
}

.ghost.danger:hover {
  color: var(--err);
  border-color: #f0cfcc;
  background: #fdf6f5;
}
</style>
