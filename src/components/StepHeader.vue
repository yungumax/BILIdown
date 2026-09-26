<script setup>
defineProps({
  steps: { type: Array, required: true },
});
const emit = defineEmits(["select"]);

/** 能跳的步骤才可点：没有可去的地方时保持普通展示 */
function go(step) {
  if (!step.to) return;
  emit("select", step.to);
}
</script>

<template>
  <ol class="steps">
    <li
      v-for="step in steps"
      :key="step.title"
      :class="[step.state, { clickable: !!step.to && step.state !== 'active' }]"
      :role="step.to ? 'button' : null"
      :title="step.to && step.state !== 'active' ? `回到「${step.title}」` : null"
      @click="go(step)"
    >
      <span class="dot">
        <svg v-if="step.state === 'done'" viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="m6.5 12.4 3.6 3.6 7.4-8"
            fill="none"
            stroke="currentColor"
            stroke-width="2.4"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        <template v-else>{{ step.index }}</template>
      </span>
      <span class="text">
        <span class="title">{{ step.title }}</span>
        <span class="hint">{{ step.hint }}</span>
      </span>
    </li>
  </ol>
</template>

<style scoped>
.steps {
  display: flex;
  gap: 34px;
  margin: 0 0 18px;
  padding: 0;
  list-style: none;
  border-bottom: 1px solid var(--line);
}

li {
  display: flex;
  align-items: center;
  gap: 10px;
  padding-bottom: 12px;
  margin-bottom: -1px;
  border-bottom: 2px solid transparent;
  color: var(--faint);
}

li.active {
  border-bottom-color: var(--accent);
  color: var(--text);
}

li.done {
  border-bottom-color: var(--accent-line);
  color: var(--muted);
}

.dot {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  font-size: 12px;
  font-weight: 600;
  border-radius: 50%;
  border: 1px solid var(--line);
  background: var(--field);
  color: var(--faint);
}

.dot svg {
  width: 14px;
  height: 14px;
}

li.clickable {
  cursor: pointer;
}

li.clickable:hover {
  color: var(--text);
}

li.clickable:hover .dot {
  border-color: var(--accent-line);
  color: var(--accent);
}

li.active .dot {
  color: #fff;
  background: var(--accent);
  border-color: var(--accent);
}

li.done .dot {
  color: var(--accent);
  background: var(--accent-soft);
  border-color: var(--accent-line);
}

.text {
  display: flex;
  flex-direction: column;
  line-height: 1.25;
}

.title {
  font-size: 13.5px;
  font-weight: 600;
}

.hint {
  font-size: 11px;
  color: var(--faint);
}
</style>
