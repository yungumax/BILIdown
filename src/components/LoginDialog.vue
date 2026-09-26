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
    width: 236,
    margin: 1,
    errorCorrectionLevel: "M",
    color: { dark: "#0f1115", light: "#ffffff" },
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
            UID {{ login.mid }}
            <span v-if="login.vip" class="vip">{{ login.vip_label || "大会员" }}</span>
          </p>
        </div>
        <p class="tip">登录后可下载 1080P 及以上清晰度，大会员可下载 4K、HDR、杜比与无损音轨。</p>
        <div class="actions">
          <button class="ghost" @click="emit('logout')">退出登录</button>
          <button class="primary" @click="emit('close')">完成</button>
        </div>
      </template>

      <template v-else>
        <div class="qr-area">
          <canvas v-show="showQr" ref="canvas"></canvas>
          <div v-if="state === 'loading'" class="spinner" aria-hidden="true"></div>
          <div v-else-if="state === 'expired'" class="expired">
            <button class="primary" @click="emit('refresh')">重新生成</button>
          </div>
        </div>
        <p class="hint" :class="{ ok: state === 'confirmed' }">{{ hint }}</p>
        <p class="tip">
          登录凭据只保存在本机，等同账号密码，请勿分享该文件。
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
  background: rgba(8, 10, 14, 0.62);
  z-index: 20;
}

.dialog {
  width: 340px;
  padding: 20px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
}

h2 {
  margin: 0;
  font-size: 14.5px;
  font-weight: 600;
}

.close {
  width: 24px;
  height: 24px;
  color: var(--faint);
  border-radius: var(--r-sm);
  font-size: 12px;
}

.close:hover {
  background: var(--hover);
  color: var(--text);
}

.qr-area {
  display: grid;
  place-items: center;
  min-height: 236px;
  background: #fff;
  border-radius: var(--r-md);
  padding: 6px;
}

.qr-area canvas {
  display: block;
}

.spinner {
  width: 26px;
  height: 26px;
  border: 2px solid #d7dae0;
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.9s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.expired {
  color: #333;
}

.hint {
  margin: 12px 0 0;
  text-align: center;
  font-size: 12.5px;
  color: var(--muted);
}

.hint.ok {
  color: var(--ok);
}

.tip {
  margin: 10px 0 0;
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--faint);
  text-align: center;
}

.account {
  padding: 14px;
  background: var(--raised);
  border-radius: var(--r-md);
  text-align: center;
}

.uname {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.sub {
  margin: 5px 0 0;
  font-size: 12px;
  color: var(--muted);
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
}

.vip {
  padding: 1px 7px;
  font-size: 11px;
  color: var(--accent);
  background: var(--accent-soft);
  border-radius: 999px;
}

.actions {
  display: flex;
  gap: 10px;
  margin-top: 16px;
}

.ghost {
  flex: 1;
  padding: 9px;
  color: var(--muted);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
}

.ghost:hover {
  color: var(--err);
  border-color: rgba(229, 84, 75, 0.5);
}

.primary {
  flex: 1;
  padding: 9px;
  font-weight: 600;
  color: #240d16;
  background: var(--accent);
  border-radius: var(--r-sm);
}

.primary:hover {
  background: #ff86a8;
}
</style>
