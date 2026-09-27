<script setup>
import { computed, onMounted, ref, watch } from "vue";
import Icon from "../components/Icon.vue";
import * as api from "../api";

const props = defineProps({
  login: { type: Object, required: true },
});
const emit = defineEmits(["goto", "toast", "open-source", "login"]);

const TAB = { fav: "fav", sub: "sub" };
const tab = ref(TAB.fav);
const data = ref(null);
const loading = ref(false);
const error = ref("");
const picked = ref(new Set());

const loggedIn = computed(() => !!props.login?.logged_in);

const folders = computed(() => (tab.value === TAB.fav ? data.value?.created ?? [] : data.value?.subscribed ?? []));

const favUrl = (id) => `https://space.bilibili.com/${data.value?.mid ?? 0}/favlist?fid=${id}`;

async function load() {
  if (!loggedIn.value || loading.value) return;
  loading.value = true;
  error.value = "";
  try {
    data.value = await api.libraryFolders();
    picked.value = new Set();
  } catch (e) {
    error.value = String(e);
    data.value = null;
  } finally {
    loading.value = false;
  }
}

function toggle(item) {
  const next = new Set(picked.value);
  if (next.has(item.id)) next.delete(item.id);
  else next.add(item.id);
  picked.value = next;
}

/** 把选中的收藏夹交给解析页：一行一个链接，那边解析后可以继续筛具体视频 */
function openPicked() {
  const list = folders.value.filter((item) => picked.value.has(item.id));
  if (!list.length) return;
  emit("open-source", list.map((item) => favUrl(item.id)).join("\n"));
}

watch(
  () => props.login?.logged_in,
  (on) => {
    if (on) load();
    else data.value = null;
  },
  { immediate: true }
);

onMounted(() => {
  if (loggedIn.value) load();
});
</script>

<template>
  <div>
    <section class="card">
      <div class="tabs">
        <button :class="{ active: tab === TAB.fav }" @click="tab = TAB.fav">收藏夹</button>
        <button :class="{ active: tab === TAB.sub }" @click="tab = TAB.sub">订阅合集</button>
      </div>

      <header class="head">
        <div>
          <h2>{{ tab === TAB.fav ? "收藏夹" : "订阅合集" }}</h2>
          <p class="hint">选择内容集合，随后可继续筛选具体视频。</p>
        </div>
        <button v-if="loggedIn" class="ghost" :disabled="loading" title="刷新" @click="load">
          <Icon name="refresh" class="btn-icon" />
        </button>
      </header>

      <div v-if="!loggedIn" class="empty">
        <Icon name="lock" class="empty-icon" />
        <div class="empty-text">
          <p class="title">登录后连接你的内容库</p>
          <p class="hint">
            登录凭据只存在本机（数据目录里的 cookies.json），内容库不会显示或导出 Cookie 的内容。
          </p>
        </div>
        <button class="primary" @click="emit('login')">登录账号</button>
      </div>

      <div v-else-if="loading && !data" class="empty">
        <div class="empty-text"><p class="title">正在读取账号里的收藏夹…</p></div>
      </div>

      <div v-else-if="error" class="empty">
        <div class="empty-text">
          <p class="title">没读出来</p>
          <p class="hint">{{ error }}</p>
        </div>
        <button class="ghost" @click="load">重试</button>
      </div>

      <template v-else>
        <ul v-if="folders.length" class="list">
          <li v-for="item in folders" :key="item.id" :class="{ on: picked.has(item.id) }">
            <label class="pick">
              <input type="checkbox" :checked="picked.has(item.id)" @change="toggle(item)" />
              <span class="meta">
                <span class="title">{{ item.title }}</span>
                <span class="sub">
                  <span v-if="item.owner">{{ item.owner }}</span>
                  <span>{{ item.media_count }} 个视频</span>
                </span>
              </span>
            </label>
            <button class="ghost" @click="emit('open-source', favUrl(item.id))">单独解析</button>
          </li>
        </ul>
        <div v-else class="empty">
          <div class="empty-text">
            <p class="title">{{ tab === TAB.fav ? "这个账号还没有收藏夹" : "还没有订阅合集" }}</p>
            <p class="hint">
              {{ tab === TAB.fav
                ? "在 B 站建一个收藏夹、往里加点视频，这里就能看到。"
                : "在 B 站订阅别人的合集或收藏夹，这里就能看到。" }}
            </p>
          </div>
        </div>

        <footer v-if="folders.length" class="foot">
          <span class="count">已选 {{ picked.size }} 个集合</span>
          <button class="primary" :disabled="!picked.size" @click="openPicked">
            <Icon name="listDetails" class="btn-icon" />
            去解析所选
          </button>
        </footer>
      </template>
    </section>
  </div>
</template>

<style scoped>
.card {
  padding: 18px 22px 20px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius-lg);
}

.tabs {
  display: flex;
  gap: 4px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--line);
}

.tabs button {
  padding: 7px 12px;
  margin-bottom: -1px;
  font-size: 13.5px;
  color: var(--muted);
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  cursor: pointer;
}

.tabs button.active {
  color: var(--accent-ink);
  border-bottom-color: var(--accent);
  font-weight: 600;
}

.head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

h2 {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
  letter-spacing: -0.2px;
}

.hint {
  margin: 6px 0 0;
  font-size: 12.5px;
  color: var(--muted);
}

.btn-icon {
  width: 16px;
  height: 16px;
}

.empty {
  display: flex;
  align-items: center;
  gap: 18px;
  margin-top: 16px;
  padding: 44px 26px;
  background: var(--raised);
  border: 1px dashed var(--line);
  border-radius: var(--radius);
}

.empty-icon {
  flex: none;
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  padding: 11px;
  color: var(--accent-ink);
  background: var(--accent-soft);
  border: 1px solid var(--accent-line);
  border-radius: 50%;
}

.empty-text {
  flex: 1;
  min-width: 0;
}

.empty-text .title {
  margin: 0;
  font-size: 14.5px;
  font-weight: 600;
}

.empty-text .hint {
  max-width: 62ch;
  line-height: 1.7;
}

.list {
  list-style: none;
  margin: 16px 0 0;
  padding: 0;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  overflow: hidden;
}

.list li {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 13px;
}

.list li + li {
  border-top: 1px solid var(--line-soft);
}

.list li:hover {
  background: var(--hover);
}

.list li.on {
  background: var(--accent-soft);
}

.pick {
  display: flex;
  flex: 1;
  align-items: center;
  gap: 11px;
  min-width: 0;
  cursor: pointer;
}

.pick input {
  flex: none;
}

.meta {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.meta .title {
  font-size: 13.5px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.meta .sub {
  display: flex;
  gap: 9px;
  margin-top: 2px;
  font-size: 11.5px;
  color: var(--faint);
}

.foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 14px;
}

.count {
  font-size: 12.5px;
  color: var(--muted);
}
</style>
