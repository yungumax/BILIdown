<script setup>
import { computed } from "vue";
import TaskRow from "../components/TaskRow.vue";

const props = defineProps({
  tasks: { type: Array, required: true },
});
const emit = defineEmits(["cancel", "open", "clear"]);

const RUNNING = ["queued", "downloading", "merging"];

const finishedCount = computed(
  () => props.tasks.filter((task) => !RUNNING.includes(task.status)).length
);

const runningCount = computed(
  () => props.tasks.filter((task) => RUNNING.includes(task.status)).length
);
</script>

<template>
  <div>
    <section class="card">
      <header class="page-head">
        <div>
          <h1>传输</h1>
          <p class="lead">下载队列与任务状态；同时最多进行 2 个任务，其余排队等待。</p>
        </div>
        <span class="spacer"></span>
        <span class="counter num">
          {{ runningCount }} 进行中
          <template v-if="finishedCount"> · {{ finishedCount }} 已结束</template>
        </span>
        <button v-if="finishedCount" class="ghost" @click="emit('clear')">
          清除已结束
        </button>
      </header>

      <div v-if="!tasks.length" class="empty">
        <p class="empty-title">队列是空的</p>
        <p class="empty-hint">到「解析」页粘贴视频链接，选择清晰度后即可开始下载。</p>
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
  </div>
</template>

<style scoped>
.card {
  padding: 20px 22px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius-lg);
}

.page-head {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  margin-bottom: 18px;
}

h1 {
  margin: 0;
  font-size: 25px;
  font-weight: 700;
  letter-spacing: -0.2px;
}

.lead {
  margin: 7px 0 0;
  font-size: 13px;
  color: var(--muted);
}

.spacer {
  flex: 1;
}

.counter {
  align-self: center;
  font-size: 12px;
  color: var(--faint);
}

.ghost {
  align-self: center;
  padding: 7px 13px;
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

.empty {
  padding: 52px 20px;
  text-align: center;
  border: 1px dashed var(--line);
  border-radius: var(--radius);
}

.empty-title {
  margin: 0;
  font-size: 13.5px;
  font-weight: 600;
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
  gap: 9px;
}
</style>
