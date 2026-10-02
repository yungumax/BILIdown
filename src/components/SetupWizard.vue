<script setup>
/**
 * 首次启动引导页：欢迎 + 设置保存目录。
 * 安装后第一次打开应用时全屏展示，完成后写 setup_done=true 不再出现。
 */
import { onMounted, ref } from "vue";
import Icon from "./Icon.vue";
import * as api from "../api";
import {
  QUALITY_CHOICES,
  AUDIO_PREFS,
  VIDEO_FORMATS,
  AUDIO_FORMATS,
  IMAGE_FORMATS,
} from "../settings-options";

const props = defineProps({
  settings: { type: Object, default: null },
  env: { type: Object, default: null },
});
const emit = defineEmits(["done"]);

const dir = ref(props.settings?.output_dir || "");
// 媒体偏好：初值即当前设置（首次安装就是后端默认值，什么都不改也能直接开始）
const quality = ref(props.settings?.default_quality ?? 0);
const audio = ref(props.settings?.default_audio || "auto");
const container = ref(props.settings?.container || "mp4");
const audioFormat = ref(props.settings?.audio_format || "source");
const imageFormat = ref(props.settings?.image_format || "source");
const cover = ref(props.settings?.download_cover ?? true);
const danmaku = ref(props.settings?.download_danmaku ?? true);
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
      default_quality: quality.value,
      default_audio: audio.value,
      container: container.value,
      audio_format: audioFormat.value,
      image_format: imageFormat.value,
      download_cover: cover.value,
      download_danmaku: danmaku.value,
      setup_done: true,
    });
    emit("done", dir.value.trim(), false);
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}

/** 跳过：一条设置都不改（目录用默认、偏好保持出厂值），只标记引导完成 */
async function skip() {
  if (saving.value) return;
  saving.value = true;
  error.value = "";
  try {
    await api.updateSettings({ ...props.settings, setup_done: true });
    emit("done", "", true);
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

      <p class="lead">开始前，先确定下载目录与默认偏好。之后都可以在「设置」里随时修改。</p>

      <div class="dir-field">
        <label>保存目录</label>
        <div class="dir-row">
          <span class="dir-value" :title="dir">{{ dir || "（未选择）" }}</span>
          <button class="ghost" :disabled="picking" @click="pickDir">
            {{ picking ? "选择中…" : "选择文件夹" }}
          </button>
        </div>
      </div>

      <div class="media-field">
        <label class="section-label">下载偏好</label>
        <div class="grid2">
          <div class="field">
            <label>视频清晰度</label>
            <select v-model.number="quality">
              <option v-for="item in QUALITY_CHOICES" :key="item.value" :value="item.value">
                {{ item.label }}
              </option>
            </select>
          </div>
          <div class="field">
            <label>音频质量</label>
            <select v-model="audio">
              <option v-for="item in AUDIO_PREFS" :key="item.value" :value="item.value">
                {{ item.label }}
              </option>
            </select>
          </div>
        </div>
        <div class="grid3">
          <div class="field">
            <label>视频格式</label>
            <select v-model="container">
              <option v-for="item in VIDEO_FORMATS" :key="item.value" :value="item.value">
                {{ item.label }}
              </option>
            </select>
          </div>
          <div class="field">
            <label>音频格式</label>
            <select v-model="audioFormat">
              <option v-for="item in AUDIO_FORMATS" :key="item.value" :value="item.value">
                {{ item.label }}
              </option>
            </select>
          </div>
          <div class="field">
            <label>图片格式</label>
            <select v-model="imageFormat">
              <option v-for="item in IMAGE_FORMATS" :key="item.value" :value="item.value">
                {{ item.label }}
              </option>
            </select>
          </div>
        </div>
        <p class="note">
          视频格式只管封装（MP4 通用、TS 给剪辑工具）；音频格式只管「音频」来源的成品，
          视频里的音轨保持原编码；图片格式只管图文图片与封面。后两项选转码需要 ffmpeg。
        </p>
        <div class="grid2 checks">
          <label class="check card-check">
            <input type="checkbox" v-model="cover" />
            <span>下载封面（独立图片）</span>
          </label>
          <label class="check card-check">
            <input type="checkbox" v-model="danmaku" />
            <span>下载弹幕（独立 .xml）</span>
          </label>
        </div>
      </div>

      <div class="env-line">
        <span class="dot" :class="{ ok: env?.ffmpeg_ok, bad: env && !env.ffmpeg_ok }"></span>
        <template v-if="env?.ffmpeg_ok">FFmpeg 就绪——音视频合成没问题</template>
        <template v-else-if="env">FFmpeg 未找到——可以在「设置 → 编码与处理」里指定路径</template>
        <template v-else>正在检测运行环境…</template>
      </div>

      <p v-if="error" class="error">{{ error }}</p>

      <div class="actions">
        <button class="primary start" :disabled="saving || !dir.trim()" @click="finish">
          {{ saving ? "保存中…" : "开始使用" }}
        </button>
        <button class="skip" :disabled="saving" @click="skip">
          跳过，稍后在设置中配置
        </button>
      </div>

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
  width: 520px;
  max-width: calc(100vw - 48px);
  max-height: calc(100vh - 40px);
  overflow-y: auto;
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

.media-field {
  margin-top: 20px;
}

.section-label {
  display: block;
  margin-bottom: 10px;
  font-size: 12.5px;
  font-weight: 600;
}

.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.grid3 {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  margin-top: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.field label {
  font-size: 12.5px;
  font-weight: 600;
}

.field select {
  width: 100%;
  padding: 7px 10px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  transition: border-color 0.15s ease;
}

.field select:hover {
  border-color: var(--accent-line);
}

.field select:focus {
  outline: none;
  border-color: var(--accent-line);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.note {
  margin: 10px 0 0;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--faint);
}

.checks {
  margin-top: 12px;
}

.check {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  cursor: pointer;
}

.check input {
  flex: none;
}

.card-check {
  padding: 10px 12px;
  background: var(--raised);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius-sm);
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

.actions {
  display: flex;
  gap: 10px;
  margin-top: 20px;
}

.start {
  flex: 1;
  padding: 10px 0;
  font-size: 14px;
  font-weight: 600;
  color: #fff;
  background: var(--accent);
  border-radius: var(--radius-sm);
}

.start:hover:not(:disabled) {
  background: var(--accent-dark);
}

.start:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.skip {
  flex: 1;
  padding: 9px 0;
  font-size: 13px;
  color: var(--muted);
  background: var(--raised);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius-sm);
  transition: border-color 0.15s ease, color 0.15s ease;
}

.skip:hover {
  border-color: var(--accent-line);
  color: var(--text);
}

.hint {
  margin: 14px 0 0;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--faint);
  text-align: center;
}
</style>
