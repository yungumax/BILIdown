<script setup>
import { computed, ref, watch } from "vue";

const props = defineProps({
  probe: { type: Object, required: true },
});
const emit = defineEmits(["start", "dismiss"]);

const quality = ref(props.probe.recommended_quality);
const audio = ref("normal");

watch(
  () => props.probe,
  (next) => {
    quality.value = next.recommended_quality;
    audio.value = "normal";
  }
);

const duration = computed(() => {
  const total = props.probe.duration;
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
    : `${m}:${String(s).padStart(2, "0")}`;
});

const bestLabel = computed(() => {
  const best = props.probe.qualities.find(
    (q) => q.qn === props.probe.best_quality && q.available
  );
  return best ? best.label : "";
});
</script>

<template>
  <article class="preview">
    <div class="thumb">
      <img v-if="probe.cover" :src="probe.cover" alt="" />
      <svg v-else class="thumb-placeholder" viewBox="0 0 24 24" aria-hidden="true">
        <rect
          x="3.2"
          y="5.4"
          width="17.6"
          height="13.2"
          rx="2.2"
          fill="none"
          stroke="currentColor"
          stroke-width="1.4"
        />
        <path d="M10.2 9.4v5.2l4.6-2.6-4.6-2.6Z" fill="currentColor" />
      </svg>
    </div>

    <div class="detail">
      <div class="title-row">
        <h2 class="title">{{ probe.title }}</h2>
        <button class="dismiss" title="收起" @click="emit('dismiss')">✕</button>
      </div>

      <dl class="facts">
        <div>
          <dt>UP 主</dt>
          <dd>{{ probe.owner }}</dd>
        </div>
        <div>
          <dt>时长</dt>
          <dd class="num">{{ duration }}</dd>
        </div>
        <div>
          <dt>BV 号</dt>
          <dd class="num">{{ probe.bvid }}</dd>
        </div>
        <div v-if="bestLabel">
          <dt>本视频最高</dt>
          <dd>{{ bestLabel }}</dd>
        </div>
      </dl>

      <p v-if="probe.note" class="note">{{ probe.note }}</p>

      <div class="controls">
        <label class="field">
          <span class="label">清晰度</span>
          <select v-model.number="quality">
            <option
              v-for="item in probe.qualities"
              :key="item.qn"
              :value="item.qn"
              :disabled="!item.available"
            >
              {{ item.label }}{{ item.hint ? `（${item.hint}）` : "" }}
            </option>
          </select>
        </label>

        <label class="field">
          <span class="label">音轨</span>
          <select v-model="audio">
            <option
              v-for="item in probe.audios"
              :key="item.kind"
              :value="item.kind"
              :disabled="!item.available"
            >
              {{ item.label }}{{ item.available ? "" : "（不可用）" }}
            </option>
          </select>
        </label>

        <button class="go" @click="emit('start', { quality, audio })">
          开始下载
        </button>
      </div>
    </div>
  </article>
</template>

<style scoped>
.preview {
  display: flex;
  gap: 16px;
  padding: 16px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}

.thumb {
  flex: none;
  display: grid;
  place-items: center;
  width: 132px;
  height: 82px;
  border-radius: var(--r-sm);
  overflow: hidden;
  background: var(--raised);
}

.thumb-placeholder {
  width: 30px;
  height: 30px;
  color: #3a4150;
}

.thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.detail {
  flex: 1;
  min-width: 0;
}

.title-row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.title {
  flex: 1;
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  line-height: 1.4;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.dismiss {
  flex: none;
  width: 22px;
  height: 22px;
  border-radius: var(--r-sm);
  color: var(--faint);
  font-size: 12px;
  line-height: 1;
}

.dismiss:hover {
  background: var(--hover);
  color: var(--text);
}

.facts {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 18px;
  margin: 9px 0 0;
}

.facts div {
  display: flex;
  align-items: baseline;
  gap: 6px;
}

.facts dt {
  color: var(--faint);
  font-size: 12px;
}

.facts dd {
  margin: 0;
  font-size: 12.5px;
}

.note {
  margin: 10px 0 0;
  padding: 6px 10px;
  font-size: 12.5px;
  color: var(--warn);
  background: rgba(224, 179, 65, 0.1);
  border-radius: var(--r-sm);
}

.controls {
  display: flex;
  align-items: flex-end;
  gap: 12px;
  margin-top: 14px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.label {
  font-size: 11.5px;
  color: var(--faint);
}

select {
  min-width: 168px;
  padding: 8px 10px;
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
}

select:hover {
  border-color: #3a4150;
}

select:focus {
  outline: none;
  border-color: var(--accent);
}

option:disabled {
  color: var(--faint);
}

.go {
  margin-left: auto;
  padding: 9px 24px;
  font-weight: 600;
  color: #240d16;
  background: var(--accent);
  border-radius: var(--r-sm);
}

.go:hover {
  background: #ff86a8;
}
</style>
