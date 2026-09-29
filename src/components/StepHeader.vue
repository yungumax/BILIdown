<script setup>
import Icon from "./Icon.vue";
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
        <Icon v-if="step.state === 'done'" name="check" />
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

/* 已完成的步骤只换打勾，不再画横线：横线只属于当前所在的步骤 */
li.done {
  color: var(--muted);
}

.dot {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  font-size: 12px;
  font-weight: 600;
  border-radius: 50%;
  border: 1px solid var(--line);
  background: var(--field);
  color: var(--faint);
  transition: transform var(--motion-fast) var(--ease-out), background var(--motion-fast) var(--ease-out);
}

.dot svg {
  width: 17px;
  height: 17px;
}

li.clickable {
  cursor: pointer;
}

li.clickable:hover {
  color: var(--text);
}

li.active .dot {
  color: #fff;
  background: var(--accent);
  border-color: var(--accent);
  /* 当前步的圆点轻轻呼吸（进行中的生命感） */
  animation: dot-breathe 1.8s var(--ease-out) infinite;
}

@keyframes dot-breathe {
  50% { opacity: 0.55; }
}

/* 步骤完成瞬间：对勾弹一下（state 换到 done 时演一次） */
li.done .dot {
  animation: dot-done 300ms var(--ease-out-expo);
}

@keyframes dot-done {
  0% { transform: scale(0.7); }
  60% { transform: scale(1.12); }
  100% { transform: none; }
}
</style>
