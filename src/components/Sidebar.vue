<script setup>
import { computed } from "vue";

const props = defineProps({
  current: { type: String, required: true },
  login: { type: Object, required: true },
  queue: { type: Object, default: () => ({ active: 0, speed: 0 }) },
});
const emit = defineEmits(["navigate"]);

const items = [
  {
    key: "parse",
    label: "解析",
    hint: "添加与选择",
    path: "M10.2 6.4H7.6a4 4 0 0 0 0 8h2.6M13.8 6.4h2.6a4 4 0 0 1 0 8h-2.6M8.6 10.4h6.8",
  },
  {
    key: "library",
    label: "内容库",
    hint: "收藏与订阅",
    path: "M7.4 4.6h9.2v14.8l-4.6-3.6-4.6 3.6Z",
  },
  {
    key: "transfer",
    label: "传输",
    hint: "队列与恢复",
    path: "M8.4 4.8v14.4M8.4 19.2 5.2 16M8.4 19.2 11.6 16M15.6 19.2V4.8M15.6 4.8 12.4 8M15.6 4.8 18.8 8",
  },
  {
    key: "settings",
    label: "设置",
    hint: "偏好与维护",
    path: "M5.2 8.4h13.6M5.2 15.6h13.6M9.6 6.6v3.6M15 13.8v3.6",
  },
  {
    key: "about",
    label: "关于",
    hint: "版本与链接",
    path: "M12 20.2a8.2 8.2 0 1 0 0-16.4 8.2 8.2 0 0 0 0 16.4ZM12 11v5.4M12 7.6v.9",
  },
];

const statusText = computed(() => {
  if (props.queue.active > 0) {
    return `下载中 ${props.queue.active} 个任务`;
  }
  return "队列空闲";
});

const speedText = computed(() => {
  const speed = props.queue.speed;
  if (!speed) return "0 B/s";
  const units = ["B", "KB", "MB", "GB"];
  let value = speed;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit > 1 ? 1 : 0)} ${units[unit]}/s`;
});
</script>

<template>
  <aside class="sidebar">
    <nav>
      <button
        v-for="item in items"
        :key="item.key"
        class="nav-item"
        :class="{ active: current === item.key }"
        @click="emit('navigate', item.key)"
      >
        <svg class="icon" viewBox="0 0 24 24" aria-hidden="true">
          <path
            :d="item.path"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        <span class="text">
          <span class="label">{{ item.label }}</span>
          <span class="hint">{{ item.hint }}</span>
        </span>
      </button>
    </nav>

    <div class="status">
      <p class="line">
        <span class="dot" :class="{ busy: queue.active > 0 }"></span>
        {{ statusText }}
      </p>
      <p class="line sub num">
        {{ speedText }} · {{ login.logged_in ? "已登录" : "未登录" }}
      </p>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  flex: none;
  width: 196px;
  display: flex;
  flex-direction: column;
  padding: 14px 12px 12px;
  background: var(--side-bg);
  border-right: 1px solid var(--line);
}

nav {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 9px 11px;
  text-align: left;
  color: var(--muted);
  border: 1px solid transparent;
  border-radius: var(--radius);
  transition: background 0.15s ease, color 0.15s ease;
}

.nav-item:hover {
  background: var(--hover);
  color: var(--text);
}

.nav-item.active {
  color: var(--accent-dark);
  background: var(--card);
  border-color: var(--accent-line);
  box-shadow: 0 1px 2px rgba(190, 120, 145, 0.08);
}

.icon {
  flex: none;
  width: 22px;
  height: 22px;
}

.nav-item.active .icon {
  color: var(--accent);
}

.text {
  display: flex;
  flex-direction: column;
  line-height: 1.3;
}

.label {
  font-size: 13.5px;
  font-weight: 600;
}

.hint {
  font-size: 11px;
  color: var(--faint);
}

.nav-item.active .hint {
  color: var(--accent);
  opacity: 0.75;
}

.status {
  padding: 11px 11px 4px;
  border-top: 1px solid var(--line);
}

.line {
  display: flex;
  align-items: center;
  gap: 7px;
  margin: 0;
  font-size: 12px;
  color: var(--text);
}

.line.sub {
  margin-top: 3px;
  padding-left: 14px;
  font-size: 11.5px;
  color: var(--faint);
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--ok);
}

.dot.busy {
  background: var(--accent);
}
</style>
