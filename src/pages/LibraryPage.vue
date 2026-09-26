<script setup>
const emit = defineEmits(["goto"]);
</script>

<template>
  <div>
    <section class="card">
      <h1>内容库</h1>
      <p class="lead">收藏夹、合集与 UP 空间浏览。</p>

      <div class="placeholder">
        <svg class="icon" viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="M7.4 4.6h9.2v14.8l-4.6-3.6-4.6 3.6Z"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linejoin="round"
          />
        </svg>
        <p class="title">这部分还没做</p>
        <p class="hint">
          计划在 M3 提供：收藏夹与合集列表、UP 空间投稿翻页、按页批量下载。
          当前可以直接把收藏夹或合集链接粘到解析页，逐条解析下载。
        </p>
        <button class="ghost" @click="emit('goto', 'parse')">去解析页</button>
      </div>
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

.placeholder {
  margin-top: 20px;
  padding: 42px 26px;
  text-align: center;
  background: var(--raised);
  border: 1px dashed var(--line);
  border-radius: var(--radius);
}

.icon {
  width: 30px;
  height: 30px;
  color: var(--faint);
}

.title {
  margin: 10px 0 0;
  font-size: 14px;
  font-weight: 600;
}

.hint {
  margin: 7px auto 0;
  max-width: 46ch;
  font-size: 12.5px;
  line-height: 1.75;
  color: var(--muted);
}

.ghost {
  margin-top: 16px;
  padding: 7px 15px;
  font-size: 12.5px;
  background: #fff;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.ghost:hover {
  border-color: #ded6da;
  background: var(--raised);
}
</style>
