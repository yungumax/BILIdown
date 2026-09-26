<script setup>
import { computed } from "vue";

const props = defineProps({
  task: { type: Object, required: true },
});
const emit = defineEmits(["cancel", "open"]);

const RUNNING = ["queued", "downloading", "merging"];
const STATUS_TEXT = {
  queued: "排队中",
  downloading: "下载中",
  merging: "合成中",
  done: "已完成",
  failed: "失败",
  canceled: "已取消",
};

/** 三个阶段各自的状态，驱动进度条与状态点 */
const stages = computed(() => {
  const task = props.task;
  const finished = task.status === "done";
  const failed = task.status === "failed";
  const canceled = task.status === "canceled";

  const build = (pct, doneByStatus, active) => {
    if (doneByStatus || pct >= 100) return { pct: 100, text: "完成", state: "done" };
    if (active) return { pct, text: `${pct.toFixed(0)}%`, state: "active" };
    if (failed) return { pct, text: "中断", state: "failed" };
    if (canceled) return { pct, text: "已取消", state: "idle" };
    return { pct, text: "待处理", state: "idle" };
  };

  return [
    {
      key: "video",
      label: "视频流",
      ...build(
        task.video_pct,
        finished,
        task.status === "downloading" && task.video_pct < 100
      ),
    },
    {
      key: "audio",
      label: "音频流",
      ...build(
        task.audio_pct,
        finished,
        task.status === "downloading" && task.video_pct >= 100
      ),
    },
    {
      key: "merge",
      label: "合成",
      ...build(finished ? 100 : 0, finished, task.status === "merging"),
    },
  ];
});

const isMergeIndeterminate = computed(() => props.task.status === "merging");

const bytesText = computed(() => {
  const task = props.task;
  if (task.status === "done") return human(task.total || task.downloaded);
  if (!task.total) return human(task.downloaded);
  return `${human(task.downloaded)} / ${human(task.total)}`;
});

const speedText = computed(() =>
  props.task.speed_bps > 0 ? `${human(props.task.speed_bps)}/s` : ""
);

const canCancel = computed(() => RUNNING.includes(props.task.status));
const canOpen = computed(() => !!props.task.output_path);

function human(bytes) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = Number(bytes) || 0;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${value} B` : `${value.toFixed(unit > 1 ? 2 : 1)} ${units[unit]}`;
}
</script>

<template>
  <li class="row" :class="task.status">
    <div class="head">
      <h4 class="title" :title="task.title">{{ task.title }}</h4>
      <span v-if="task.quality_label" class="quality">{{ task.quality_label }}</span>
      <span class="spacer"></span>
      <span class="status">{{ STATUS_TEXT[task.status] }}</span>
      <button v-if="canCancel" class="action" @click="emit('cancel')">取消</button>
      <button v-else-if="canOpen" class="action" @click="emit('open', task.output_path)">
        {{ task.status === "done" ? "打开" : "定位" }}
      </button>
    </div>

    <div class="bar">
      <div
        v-for="(stage, index) in stages"
        :key="stage.key"
        class="seg"
        :class="[stage.state, index === 2 && isMergeIndeterminate ? 'indeterminate' : '']"
      >
        <span class="fill" :style="{ width: `${stage.pct}%` }"></span>
      </div>
    </div>

    <div class="meta">
      <ul class="stages">
        <li v-for="stage in stages" :key="stage.key" :class="stage.state">
          <span class="pip"></span>
          <span class="name">{{ stage.label }}</span>
          <span class="value num">{{ stage.text }}</span>
        </li>
      </ul>
      <span class="spacer"></span>
      <span v-if="task.status === 'failed'" class="message error" :title="task.message">
        {{ task.message }}
      </span>
      <span class="bytes num">{{ bytesText }}</span>
      <span v-if="speedText" class="speed num">{{ speedText }}</span>
    </div>
  </li>
</template>

<style scoped>
.row {
  padding: 13px 15px;
  background: #fff;
  border: 1px solid var(--line);
  border-radius: var(--radius);
}

.row.done {
  border-color: #cfe9dc;
  background: #fbfefc;
}

.row.failed {
  border-color: #f0d3d0;
  background: #fdf6f5;
}

.head {
  display: flex;
  align-items: center;
  gap: 9px;
}

.title {
  margin: 0;
  max-width: 46%;
  font-size: 13.5px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quality {
  flex: none;
  padding: 1px 7px;
  font-size: 11.5px;
  color: var(--muted);
  background: var(--raised);
  border: 1px solid var(--line-soft);
  border-radius: 5px;
}

.spacer {
  flex: 1;
}

.status {
  font-size: 12px;
  color: var(--muted);
}

.row.downloading .status,
.row.merging .status {
  color: var(--accent-dark);
}

.row.done .status {
  color: var(--ok);
}

.row.failed .status {
  color: var(--err);
}

.action {
  font-size: 12px;
  color: var(--muted);
  padding: 3px 10px;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  background: #fff;
}

.action:hover {
  color: var(--text);
  border-color: #ded6da;
  background: var(--raised);
}

/* 三段式进度：视频流 | 音频流 | 合成 */
.bar {
  display: flex;
  gap: 3px;
  height: 6px;
  margin: 11px 0 9px;
}

.seg {
  flex: 1;
  background: #f0ebee;
  border-radius: 3px;
  overflow: hidden;
}

.fill {
  display: block;
  height: 100%;
  width: 0;
  background: var(--accent);
  border-radius: 3px;
  transition: width 0.25s ease;
}

.seg.done .fill {
  background: var(--ok);
}

.seg.idle .fill {
  background: #cfc6cb;
}

.row.failed .seg .fill {
  background: var(--err);
  opacity: 0.55;
}

/* 合成阶段没有百分比，用流动填充表示进行中 */
.seg.indeterminate .fill {
  width: 100%;
  background: linear-gradient(
    90deg,
    var(--accent-soft) 0%,
    var(--accent) 50%,
    var(--accent-soft) 100%
  );
  background-size: 200% 100%;
  animation: sweep 1.3s linear infinite;
}

@keyframes sweep {
  from {
    background-position: 100% 0;
  }
  to {
    background-position: -100% 0;
  }
}

.meta {
  display: flex;
  align-items: center;
  gap: 14px;
  font-size: 12px;
  color: var(--muted);
}

.stages {
  display: flex;
  gap: 14px;
  list-style: none;
  margin: 0;
  padding: 0;
}

.stages li {
  display: flex;
  align-items: center;
  gap: 5px;
}

.pip {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #cfc6cb;
}

.stages li.active .pip {
  background: var(--accent);
}

.stages li.done .pip {
  background: var(--ok);
}

.stages li.failed .pip {
  background: var(--err);
}

.stages .name {
  color: var(--faint);
}

.stages .value {
  color: var(--text);
}

.stages li.idle .value {
  color: var(--faint);
}

.bytes {
  color: var(--text);
}

.message.error {
  max-width: 40%;
  color: var(--err);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
