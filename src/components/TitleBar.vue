<script setup>
import * as api from "../api";

defineProps({
  login: { type: Object, required: true },
  version: { type: String, default: "" },
});
const emit = defineEmits(["login"]);

/** 只有按住左键才当作拖动窗口；按钮区域不参与 */
function onDrag(event) {
  if (event.buttons !== 1) return;
  event.preventDefault();
  api.startWindowDrag();
}
</script>

<template>
  <header class="titlebar">
    <div class="brand" @mousedown="onDrag">
      <svg class="mark" viewBox="0 0 24 24" aria-hidden="true">
        <rect width="24" height="24" rx="5.5" fill="#fb7299" />
        <path d="M12 6.6v6.2" stroke="#fff" stroke-width="2.2" stroke-linecap="round" />
        <path
          d="M8.4 10.6 12 14.2l3.6-3.6"
          fill="none"
          stroke="#fff"
          stroke-width="2.2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
        <path d="M7.8 17.6h8.4" stroke="#fff" stroke-width="2.2" stroke-linecap="round" />
      </svg>
      <span class="name">BILIdown</span>
      <span class="sub">
        Bilibili Download Lab
        <span v-if="version" class="ver num">v{{ version }}</span>
      </span>
    </div>

    <div class="drag-fill" @mousedown="onDrag"></div>

    <button class="login-chip" :class="{ on: login.logged_in }" @click="emit('login')">
      <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="8.5" r="3.4" fill="none" stroke="currentColor" stroke-width="1.7" />
        <path
          d="M5.6 19.2c.9-3.1 3.4-4.7 6.4-4.7s5.5 1.6 6.4 4.7"
          fill="none"
          stroke="currentColor"
          stroke-width="1.7"
          stroke-linecap="round"
        />
      </svg>
      <span class="label">{{ login.logged_in ? login.uname : "未登录" }}</span>
      <svg class="caret" viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="m7 10 5 5 5-5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>

    <div class="window-controls">
      <button class="ctrl" title="最小化" @click="api.minimizeWindow()">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M5.5 12h13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
      </button>
      <button class="ctrl" title="最大化 / 还原" @click="api.toggleMaximizeWindow()">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <rect
            x="6"
            y="6"
            width="12"
            height="12"
            rx="1.6"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
          />
        </svg>
      </button>
      <button class="ctrl close" title="关闭" @click="api.closeWindow()">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="m7 7 10 10M17 7 7 17"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
          />
        </svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  flex: none;
  display: flex;
  align-items: center;
  height: 46px;
  padding-left: 14px;
  background: var(--card);
  border-bottom: 1px solid var(--line);
  user-select: none;
}

.brand {
  display: flex;
  align-items: center;
  gap: 9px;
  cursor: default;
}

.mark {
  width: 22px;
  height: 22px;
}

.name {
  font-size: 14.5px;
  font-weight: 700;
  letter-spacing: 0.2px;
}

.sub {
  font-size: 11.5px;
  color: var(--faint);
}

.ver {
  margin-left: 2px;
}

.drag-fill {
  flex: 1;
  align-self: stretch;
}

.login-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 11px;
  margin-right: 12px;
  font-size: 12.5px;
  color: var(--accent);
  background: #fff;
  border: 1px solid var(--accent-line);
  border-radius: 999px;
  transition: background 0.15s ease;
}

.login-chip:hover {
  background: var(--accent-soft);
}

.login-chip.on {
  color: var(--text);
  border-color: var(--line);
}

.login-chip .glyph {
  width: 15px;
  height: 15px;
}

.login-chip.on .glyph {
  color: var(--accent);
}

.caret {
  width: 13px;
  height: 13px;
  color: var(--faint);
}

.window-controls {
  display: flex;
  align-items: stretch;
  height: 100%;
}

.ctrl {
  width: 44px;
  display: grid;
  place-items: center;
  color: var(--muted);
}

.ctrl svg {
  width: 17px;
  height: 17px;
}

.ctrl:hover {
  background: #f2eef0;
  color: var(--text);
}

.ctrl.close:hover {
  background: #e5484d;
  color: #fff;
}
</style>
