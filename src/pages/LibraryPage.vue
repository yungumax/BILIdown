<script setup>
import { computed, ref } from "vue";
import Icon from "../components/Icon.vue";
import * as api from "../api";

const props = defineProps({
  settings: { type: Object, default: null },
});
const emit = defineEmits(["goto", "toast", "open-source", "refresh"]);

const url = ref("");
const adding = ref(false);

const library = computed(() => props.settings?.library ?? []);

/** 来源类型 → 图标（与解析页「支持来源」那排同一套名字） */
const KIND_ICONS = {
  collection: "books",
  fav: "star",
  series: "listLine",
  space: "user",
  opus: "photo",
  audio: "microphone",
};
const KIND_LABELS = {
  collection: "合集",
  fav: "收藏夹",
  series: "系列",
  space: "UP 空间",
  opus: "图文",
  audio: "音频",
};

const kindIcon = (kind) => KIND_ICONS[kind] ?? "books";
const kindLabel = (kind) => KIND_LABELS[kind] ?? kind;

async function add() {
  const value = url.value.trim();
  if (!value || adding.value) return;
  adding.value = true;
  try {
    const entry = await api.libraryAdd(value);
    url.value = "";
    emit("refresh"); // 让 App 重读设置，列表才会立刻出现这一条
    emit("toast", `已存入内容库：${entry.title || entry.url}`);
  } catch (error) {
    emit("toast", String(error));
  } finally {
    adding.value = false;
  }
}

async function remove(item) {
  try {
    await api.libraryRemove((item.key || item.url).trim());
    emit("refresh");
    emit("toast", `已移出内容库：${item.title || item.url}`);
  } catch (error) {
    emit("toast", String(error));
  }
}
</script>

<template>
  <div>
    <section class="card">
      <h1>内容库</h1>
      <p class="lead">
        把收藏夹、合集、系列、UP 空间、图文与音频的来源存下来，随时一键解析。
        单条视频不用存，直接去解析页贴就行。
      </p>

      <div class="add-row">
        <input
          v-model="url"
          spellcheck="false"
          placeholder="粘贴收藏夹 / 合集 / 系列 / UP 空间 / 图文 / 音频的链接"
          @keydown.enter="add"
        />
        <button class="primary" :disabled="adding || !url.trim()" @click="add">
          <Icon name="plus" class="btn-icon" />
          {{ adding ? "识别中…" : "存入内容库" }}
        </button>
      </div>

      <div v-if="library.length" class="list">
        <article v-for="item in library" :key="item.key || item.url" class="row">
          <Icon :name="kindIcon(item.kind)" class="row-icon" />
          <div class="meta">
            <p class="title">{{ item.title || item.url }}</p>
            <p class="sub">
              <span class="tag">{{ kindLabel(item.kind) }}</span>
              <span v-if="item.owner">{{ item.owner }}</span>
              <span v-if="item.total">{{ item.total }} 条</span>
            </p>
          </div>
          <div class="acts">
            <button class="ghost" @click="emit('open-source', item.url)">去解析</button>
            <button
              class="ghost danger"
              :title="`移出内容库：${item.title || item.url}`"
              @click="remove(item)"
            >
              <Icon name="close" class="btn-icon" />
            </button>
          </div>
        </article>
      </div>

      <div v-else class="placeholder">
        <Icon name="books" class="icon" />
        <p class="title">内容库还空着</p>
        <p class="hint">
          把收藏夹或合集链接贴到上面的输入框，识别通过后就会存在这里（跟着设置一起落盘）。
          之后点「去解析」会自动跳到解析页并开始解析。
        </p>
        <button class="ghost" @click="emit('goto', 'parse')">先去解析页看看</button>
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
  max-width: 74ch;
  font-size: 13px;
  color: var(--muted);
}

.add-row {
  display: flex;
  gap: 10px;
  margin-top: 16px;
}

.add-row input {
  flex: 1;
  min-width: 0;
  padding: 9px 11px;
  font: inherit;
  font-size: 13px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius);
}

.add-row input:focus-visible {
  outline: none;
  border-color: var(--accent-line);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.btn-icon {
  width: 16px;
  height: 16px;
}

.list {
  display: flex;
  flex-direction: column;
  margin-top: 16px;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  overflow: hidden;
}

.row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 11px 13px;
}

.row + .row {
  border-top: 1px solid var(--line-soft);
}

.row:hover {
  background: var(--hover);
}

.row-icon {
  flex: none;
  width: 20px;
  height: 20px;
  color: var(--muted);
}

.meta {
  flex: 1;
  min-width: 0;
}

.title {
  margin: 0;
  font-size: 13.5px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sub {
  display: flex;
  align-items: center;
  gap: 9px;
  margin: 3px 0 0;
  font-size: 11.5px;
  color: var(--faint);
}

.tag {
  padding: 1px 6px;
  color: var(--accent-ink);
  background: var(--accent-soft);
  border: 1px solid var(--accent-line);
  border-radius: 5px;
}

.acts {
  display: flex;
  flex: none;
  align-items: center;
  gap: 6px;
}

.acts .danger {
  padding: 6px 8px;
  color: var(--muted);
}

.acts .danger:hover {
  color: var(--err);
  border-color: var(--fail-line);
}

.placeholder {
  margin-top: 16px;
  padding: 42px 26px;
  text-align: center;
  background: var(--raised);
  border: 1px dashed var(--line);
  border-radius: var(--radius);
}

.placeholder .icon {
  width: 34px;
  height: 34px;
  color: var(--faint);
}

.placeholder .title {
  margin: 12px 0 0;
  font-size: 14px;
  font-weight: 600;
}

.placeholder .hint {
  max-width: 52ch;
  margin: 7px auto 0;
  font-size: 12.5px;
  line-height: 1.7;
  color: var(--muted);
}

.placeholder button {
  margin-top: 16px;
}
</style>
