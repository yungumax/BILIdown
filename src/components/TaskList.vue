<script setup>
import TaskRow from "./TaskRow.vue";

const props = defineProps({
  tasks: { type: Array, required: true },
});
const emit = defineEmits(["cancel", "open", "clear"]);

const running = ["queued", "downloading", "merging"];

function hasFinished() {
  return props.tasks.some((t) => !running.includes(t.status));
}
</script>

<template>
  <section class="tasks">
    <header class="head">
      <h3>下载任务</h3>
      <span v-if="tasks.length" class="count num">{{ tasks.length }}</span>
      <span class="spacer"></span>
      <button v-if="hasFinished()" class="ghost" @click="emit('clear')">
        清除已结束
      </button>
    </header>

    <div v-if="!tasks.length" class="empty">
      <p class="empty-title">还没有任务</p>
      <p class="empty-hint">把视频链接粘到上面，解析后选好清晰度就能开始。</p>
    </div>

    <ul v-else class="list">
      <TaskRow
        v-for="task in tasks"
        :key="task.id"
        :task="task"
        @cancel="emit('cancel', task.id)"
        @open="emit('open', $event)"
      />
    </ul>
  </section>
</template>

<style scoped>
.tasks {
  margin-top: 18px;
}

.head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 0 2px 8px;
}

h3 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--muted);
}

.count {
  font-size: 12px;
  color: var(--faint);
}

.spacer {
  flex: 1;
}

.ghost {
  font-size: 12px;
  color: var(--muted);
  padding: 4px 8px;
  border-radius: var(--r-sm);
}

.ghost:hover {
  background: var(--hover);
  color: var(--text);
}

.empty {
  padding: 44px 20px;
  text-align: center;
  border: 1px dashed var(--line);
  border-radius: var(--r-lg);
}

.empty-title {
  margin: 0;
  font-size: 13.5px;
  color: var(--muted);
}

.empty-hint {
  margin: 6px 0 0;
  font-size: 12.5px;
  color: var(--faint);
}

.list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
</style>
