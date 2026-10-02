<script setup>
/**
 * 首次启动引导页：欢迎 + 设置保存目录。
 * 安装后第一次打开应用时全屏展示，完成后写 setup_done=true 不再出现。
 */
import { onMounted, ref } from "vue";
import Icon from "./Icon.vue";
import * as api from "../api";

const props = defineProps({
  settings: { type: Object, default: null },
  env: { type: Object, default: null },
});
const emit = defineEmits(["done"]);

const dir = ref(props.settings?.output_dir || "");
const picking = ref(false);
const saving = ref(false);
const error = ref("");

async function pickDir() {
  if (picking.value) return;
  picking.value = true;
  error.value = "";
  try {
    const picked = await api.chooseOutputDir();
    if (picked) dir.value = picked;
  } catch (e) {
    error.value = String(e);
  } finally {
    picking.value = false;
  }
}

async function finish() {
  if (saving.value || !dir.value.trim()) return;
  saving.value = true;
  error.value = "";
  try {
    // 复用完整保存：带 setup_done=true 写回整个 settings
    await api.updateSettings({
      ...props.settings,
      output_dir: dir.value.trim(),
      setup_done: true,
    });
    emit("done", dir.value.trim());
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}

/** 入场编排：内容块依次浮起 */
onMounted(() => {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  requestAnimationFrame(() => {
    const card = document.querySelector(".setup-card");
    if (!card) return;
    const kids = [...card.children];
    import("animejs").then(({ animate, stagger }) => {
      animate(kids, {
        opacity: [0, 1],
        translateY: [12, 0],
        duration: 520,
        delay: stagger(90),
        ease: "outExpo",
      });
    });
  });
});
</script>

<template>
  <div class="setup-backdrop">
    <div class="setup-card">
      <div class="welcome">
        <Icon name="download" class="logo" />
        <div>
          <h1>欢迎使用 BILIdown</h1>
          <p class="brand-sub">Bilibili Download Lab</p>
        </div>
      </div>

      <p class="lead">开始前，先确定下载文件保存在哪里。之后可以在「设置 → 下载」里随时修改。</p>

      <div class="dir-field">
        <label>保存目录</label>
        <div class="dir-row">
          <span class="dir-value" :title="dir">{{ dir || "（未选择）" }}</span>
          <button class="ghost" :disabled="picking" @click="pickDir">
            {{ picking ? "选择中…" : "选择文件夹" }}
          </button>
        </div>
      </div>

      <div class="env-line">
        <span class="dot" :class="{ ok: env?.ffmpeg_ok, bad: env && !env.ffmpeg_ok }"></span>
        <template v-if="env?.ffmpeg_ok">FFmpeg 就绪——音视频合成没问题</template>
        <template v-else-if="env">FFmpeg 未找到——可以在「设置 → 编码与处理」里指定路径</template>
        <template v-else>正在检测运行环境…</template>
      </div>

      <p v-if="error" class="error">{{ error }}</p>

      <button class="primary start" :disabled="saving || !dir.trim()" @click="finish">
        {{ saving ? "保存中…" : "开始使用" }}
      </button>

      <p class="hint">登录 B 站账号后可下载 1080P 及以上清晰度；不登录也能下载 480P。</p>
    </div>
  </div>
</template>

<style scoped>
.setup-backdrop {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: center;
  background: var(--app-bg);
  z-index: 40;
  animation: setup-fade 320ms var(--ease-out) both;
}

@keyframes setup-fade {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.setup-card {
  width: 460px;
  max-width: calc(100vw - 48px);
  padding: 30px 32px 26px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius-lg);
}

.welcome {
  display: flex;
  align-items: center;
  gap: 14px;
}

.logo {
  flex: none;
  width: 40px;
  height: 40px;
  padding: 8px;
  color: var(--accent);
  background: var(--accent-soft);
  border: 1px solid var(--accent-line);
  border-radius: var(--radius);
}

h1 {
  margin: 0;
  font-size: 21px;
  font-weight: 700;
  letter-spacing: -0.2px;
}

.brand-sub {
  margin: 3px 0 0;
  font-size: 12px;
  color: var(--faint);
}

.lead {
  margin: 18px 0 0;
  font-size: 13px;
  line-height: 1.65;
  color: var(--muted);
}

.dir-field {
  margin-top: 18px;
}

.dir-field label {
  display: block;
  margin-bottom: 7px;
  font-size: 12.5px;
  font-weight: 600;
}

.dir-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.dir-value {
  flex: 1;
  min-width: 0;
  padding: 8px 12px;
  font-size: 12.5px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.env-line {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 14px;
  font-size: 12px;
  color: var(--faint);
}

.dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--seg-idle);
}

.dot.ok {
  background: var(--ok);
}

.dot.bad {
  background: var(--warn);
}

.error {
  margin: 12px 0 0;
  font-size: 12px;
  color: var(--err);
}

.start {
  width: 100%;
  margin-top: 20px;
  padding: 10px 0;
  font-size: 14px;
  font-weight: 600;
}

.hint {
  margin: 14px 0 0;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--faint);
  text-align: center;
}
</style>
