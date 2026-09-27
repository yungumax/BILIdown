<script setup>
import { computed, onMounted, ref, watch } from "vue";
import Icon from "../components/Icon.vue";
import * as api from "../api";
import { enqueueBatch, pickDefaultAudio, pickDefaultQuality } from "../download-request.js";

const props = defineProps({
  login: { type: Object, required: true },
  settings: { type: Object, default: null },
});
const emit = defineEmits(["toast", "login", "open-source"]);

const TAB = { fav: "fav", sub: "sub" };
const tab = ref(TAB.fav);
const loading = ref(false);
const error = ref("");
const account = ref(null);
const query = ref("");
const picked = ref(new Set());

/** 打开的集合：当前页的条目、分页与勾选 */
const opened = ref(null);

const loggedIn = computed(() => !!props.login?.logged_in);

const folders = computed(() => {
  const list = tab.value === TAB.fav ? account.value?.created ?? [] : account.value?.subscribed ?? [];
  const keyword = query.value.trim().toLowerCase();
  if (!keyword) return list;
  return list.filter((item) =>
    [item.title, item.owner, item.intro].some((text) => (text ?? "").toLowerCase().includes(keyword))
  );
});

/** 秒 → m:ss（视频卡片右下角的时长） */
function length(seconds) {
  const total = Math.max(0, Math.round(seconds));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

/** B 站封面地址：换 https（webview 里 http 会被拦），并要一个小的缩放变体。
 *  还要配 referrerpolicy="no-referrer"：不然 CDN 拿热链直接 403。 */
function thumb(url, width = 320, height = 200) {
  if (!url) return "";
  const https = url.replace(/^http:/, "https:");
  if (https.includes("@")) return https;
  return `${https}@${width}w_${height}h_1c.webp`;
}

const favUrl = (id) => `https://space.bilibili.com/${account.value?.mid ?? 0}/favlist?fid=${id}`;

async function load() {
  if (!loggedIn.value || loading.value) return;
  loading.value = true;
  error.value = "";
  try {
    account.value = await api.libraryFolders();
    picked.value = new Set();
  } catch (e) {
    error.value = String(e);
    account.value = null;
  } finally {
    loading.value = false;
  }
}

function togglePick(id) {
  const next = new Set(picked.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  picked.value = next;
}

/** 打开集合：每页多少条由接口决定（收藏夹 20），翻页就是按起点再探一次 */
async function openFolder(folder) {
  opened.value = { folder, probe: null, items: [], page: 1, perPage: 20, total: 0, picked: new Set(), loading: true };
  await loadPage(1);
}

async function loadPage(page) {
  const view = opened.value;
  if (!view) return;
  view.loading = true;
  try {
    const perPage = view.perPage || 20;
    const probe = await api.probeRange(favUrl(view.folder.id), (page - 1) * perPage + 1);
    view.probe = probe;
    view.items = probe.items ?? [];
    // 每页多少条以接口实际给的为准，但**只增不减**：最后一页往往只有几条，
    // 拿它当页长会把页数算歪（实测冒出过"第 2 / 131 页"）
    if (view.items.length > view.perPage) view.perPage = view.items.length;
    view.total = probe.total || view.items.length;
    view.page = page;
    view.picked = new Set();
  } catch (error) {
    emit("toast", String(error));
  } finally {
    view.loading = false;
  }
}

const pageCount = computed(() => {
  const view = opened.value;
  if (!view || !view.perPage) return 1;
  return Math.max(1, Math.ceil(view.total / view.perPage));
});

/** 这一页第 index 条在来源里的序号（1 = 最旧），与解析页同一套算法 */
const absoluteOf = (index) => {
  const view = opened.value;
  const position = (view.page - 1) * view.perPage + index;
  return Math.max(view.total - position, 1);
};

function toggleItem(index) {
  const view = opened.value;
  const next = new Set(view.picked);
  if (next.has(index)) next.delete(index);
  else next.add(index);
  view.picked = next;
}

function selectPage() {
  const view = opened.value;
  view.picked =
    view.picked.size === view.items.length ? new Set() : new Set(view.items.map((_, index) => index));
}

async function downloadPicked(all = false) {
  const view = opened.value;
  if (!view?.probe) return;
  const indexes = all ? view.items.map((_, index) => index) : [...view.picked].sort((a, b) => a - b);
  const entries = indexes.map((index) => view.items[index]).filter(Boolean);
  if (!entries.length) {
    emit("toast", "先点一下要下载的内容");
    return;
  }
  const started = await enqueueBatch(view.probe, entries, {
    quality: pickDefaultQuality(view.probe, props.settings),
    audio: pickDefaultAudio(view.probe, props.settings),
    // 序号按它在来源里的位置算（分页只影响从哪一条开始显示）
    absoluteOf: (index) => absoluteOf(indexes[index]),
    onError: (entry, error) => emit("toast", `${entry.title}: ${error}`),
  });
  if (started) emit("toast", `已加入 ${started} 个下载任务`);
}

/** 选中若干个集合 → 交给解析页按各自的 favlist 解析（那边继续筛、还能改清晰度） */
function openPicked() {
  const list = [...folders.value].filter((item) => picked.value.has(item.id));
  if (!list.length) return;
  emit("open-source", list.map((item) => favUrl(item.id)).join("\n"));
}

watch(
  () => props.login?.logged_in,
  (on) => {
    if (on) load();
    else {
      account.value = null;
      opened.value = null;
    }
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
        <button :class="{ active: tab === TAB.fav }" @click="tab = TAB.fav">
          收藏夹
          <span v-if="account" class="badge">{{ account.created.length }}</span>
        </button>
        <button :class="{ active: tab === TAB.sub }" @click="tab = TAB.sub">
          订阅合集
          <span v-if="account" class="badge">{{ account.subscribed.length }}</span>
        </button>
      </div>

      <!-- 未登录：登录后连接内容库 -->
      <div v-if="!loggedIn" class="empty tall">
        <Icon name="lock" class="empty-icon" />
        <div class="empty-text">
          <p class="title">登录后连接你的内容库</p>
          <p class="hint">
            登录凭据只存在本机（数据目录里的 cookies.json），内容库不会显示或导出 Cookie 的内容。
          </p>
        </div>
        <button class="primary" @click="emit('login')">登录账号</button>
      </div>

      <!-- 集合列表 -->
      <template v-else-if="!opened">
        <header class="head">
          <div>
            <h2>{{ tab === TAB.fav ? "收藏夹" : "订阅合集" }}</h2>
            <p class="meta">
              {{ tab === TAB.fav ? "我创建的" : "我收藏的" }} · 共
              {{ (tab === TAB.fav ? account?.created.length : account?.subscribed.length) ?? 0 }} 个内容集合
            </p>
          </div>
          <button class="ghost" :disabled="loading" title="刷新" @click="load">
            <Icon name="refresh" class="btn-icon" />
          </button>
        </header>

        <div class="search">
          <Icon name="search" class="search-icon" />
          <input v-model="query" spellcheck="false" placeholder="搜索标题、创建者或简介" />
        </div>

        <p v-if="loading && !account" class="hint pad">正在读取账号里的内容集合…</p>
        <div v-else-if="error" class="empty">
          <div class="empty-text">
            <p class="title">没读出来</p>
            <p class="hint">{{ error }}</p>
          </div>
          <button class="ghost" @click="load">重试</button>
        </div>

        <div v-else-if="folders.length" class="cards">
          <article
            v-for="item in folders"
            :key="item.id"
            class="collection"
            :class="{ on: picked.has(item.id) }"
            @click="openFolder(item)"
          >
            <div class="thumb">
              <img v-if="item.cover" :src="thumb(item.cover, 240, 160)" alt="" loading="lazy" referrerpolicy="no-referrer" />
              <div v-else class="thumb-count">
                <Icon name="bookmark" />
                <b>{{ item.media_count }}</b>
              </div>
            </div>
            <div class="body">
              <p class="ctitle">{{ item.title }}</p>
              <p class="sub">{{ item.owner || "我的收藏夹" }}</p>
              <p v-if="item.intro" class="intro">{{ item.intro }}</p>
              <p class="num">{{ item.media_count }} 个视频</p>
            </div>
            <button
              class="pick"
              :title="picked.has(item.id) ? '取消选择' : '选择这个集合'"
              @click.stop="togglePick(item.id)"
            >
              <Icon :name="picked.has(item.id) ? 'check' : 'plus'" />
            </button>
          </article>
        </div>

        <div v-else class="empty">
          <div class="empty-text">
            <p class="title">
              {{ query ? "没有匹配的内容集合" : tab === TAB.fav ? "这个账号还没有收藏夹" : "还没有订阅合集" }}
            </p>
            <p class="hint">
              {{
                query
                  ? "换个关键词试试。"
                  : tab === TAB.fav
                    ? "在 B 站建一个收藏夹、往里加点视频，这里就能看到。"
                    : "在 B 站订阅别人的合集或收藏夹，这里就能看到。"
              }}
            </p>
          </div>
        </div>

        <footer v-if="folders.length" class="foot">
          <span class="count">已选 {{ picked.size }} 个集合</span>
          <button class="primary" :disabled="!picked.size" @click="openPicked">解析所选集合</button>
        </footer>
      </template>

      <!-- 集合详情：一页一页看视频 -->
      <template v-else>
        <header class="head detail">
          <button class="ghost back" title="返回" @click="opened = null">
            <Icon name="chevronLeft" class="btn-icon" />
          </button>
          <h2 class="grow">{{ opened.folder.title }}</h2>
          <span class="loaded">已加载 {{ opened.items.length }} / {{ opened.total }} 项</span>
          <button class="ghost" :disabled="opened.loading" @click="loadPage(opened.page)">
            <Icon name="refresh" class="btn-icon" />
          </button>
          <button class="ghost" :disabled="!opened.items.length" @click="downloadPicked(true)">下载全部</button>
          <button class="primary" :disabled="!opened.picked.size" @click="downloadPicked(false)">
            下载所选（{{ opened.picked.size }}）
          </button>
        </header>

        <p v-if="opened.loading" class="hint pad">读取中…</p>
        <div v-else-if="opened.items.length" class="videos">
          <article
            v-for="(item, index) in opened.items"
            :key="item.bvid || item.opus_id || index"
            class="video"
            :class="{ on: opened.picked.has(index) }"
            @click="toggleItem(index)"
          >
            <div class="cover">
              <img v-if="item.pic" :src="thumb(item.pic)" alt="" loading="lazy" referrerpolicy="no-referrer" />
              <span class="seq">{{ absoluteOf(index) }}</span>
              <span v-if="item.duration" class="len">{{ length(item.duration) }}</span>
            </div>
            <p class="vtitle">{{ item.title }}</p>
            <div class="vrow">
              <span class="owner">{{ item.owner }}</span>
              <span class="dl">{{ opened.picked.has(index) ? "已选" : "下载" }}</span>
            </div>
          </article>
        </div>
        <p v-else class="hint pad">这个集合里没有可解析的内容。</p>

        <footer v-if="opened.items.length" class="pager">
          <span class="count">已选 {{ opened.picked.size }} / {{ opened.items.length }}</span>
          <div class="pages">
            <button class="ghost" :disabled="opened.page <= 1" @click="loadPage(1)">«</button>
            <button class="ghost" :disabled="opened.page <= 1" @click="loadPage(opened.page - 1)">‹</button>
            <span class="page-now">{{ opened.page }}</span>
            <button class="ghost" :disabled="opened.page >= pageCount" @click="loadPage(opened.page + 1)">›</button>
            <button class="ghost" :disabled="opened.page >= pageCount" @click="loadPage(pageCount)">»</button>
            <input
              class="jump"
              type="number"
              min="1"
              :max="pageCount"
              :value="opened.page"
              title="跳到第几页（回车）"
              @keydown.enter="loadPage(Math.min(Math.max(1, Number($event.target.value) || 1), pageCount))"
            />
            <span class="meta">第 {{ opened.page }} / {{ pageCount }} 页</span>
          </div>
          <button class="ghost" @click="selectPage">
            {{ opened.picked.size === opened.items.length ? "取消本页" : "全选本页" }}
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
  gap: 18px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--line);
}

.tabs button {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
  padding: 7px 0 9px;
  margin-bottom: -1px;
  font-size: 13.5px;
  color: var(--muted);
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  cursor: pointer;
}

.tabs button.active {
  font-weight: 600;
  color: var(--accent-ink);
  border-bottom-color: var(--accent);
}

.badge {
  font-size: 11.5px;
  color: var(--faint);
}

.head {
  display: flex;
  align-items: center;
  gap: 10px;
}

.head.detail {
  padding-bottom: 12px;
  border-bottom: 1px solid var(--line-soft);
}

.grow {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

h2 {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
  letter-spacing: -0.2px;
}

.meta {
  margin: 5px 0 0;
  font-size: 12.5px;
  color: var(--muted);
}

.loaded {
  flex: none;
  font-size: 12.5px;
  color: var(--muted);
}

.btn-icon {
  width: 16px;
  height: 16px;
}

.search {
  position: relative;
  margin-top: 14px;
}

.search-icon {
  position: absolute;
  top: 50%;
  left: 11px;
  width: 16px;
  height: 16px;
  color: var(--faint);
  transform: translateY(-50%);
}

.search input {
  width: 100%;
  padding: 9px 12px 9px 34px;
  font: inherit;
  font-size: 13px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius);
}

.search input:focus-visible {
  outline: none;
  border-color: var(--accent-line);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.pad {
  margin: 16px 0 0;
}

.cards {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
  margin-top: 16px;
}

.collection {
  position: relative;
  display: flex;
  gap: 13px;
  padding: 12px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  cursor: pointer;
}

.collection:hover {
  border-color: var(--accent-line);
}

.collection.on {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.thumb {
  flex: none;
  width: 92px;
  height: 62px;
  overflow: hidden;
  background: var(--thumb);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius-sm);
}

.thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.thumb-count {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  height: 100%;
  color: var(--accent-ink);
}

.thumb-count svg {
  width: 20px;
  height: 20px;
}

.thumb-count b {
  font-size: 15px;
}

.body {
  flex: 1;
  min-width: 0;
}

.ctitle {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sub {
  margin: 4px 0 0;
  font-size: 11.5px;
  color: var(--faint);
}

.intro {
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow: hidden;
  margin: 4px 0 0;
  font-size: 11.5px;
  color: var(--muted);
}

.num {
  margin: 6px 0 0;
  font-size: 11.5px;
  color: var(--faint);
}

.pick {
  position: absolute;
  top: 10px;
  right: 10px;
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  color: var(--muted);
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: 50%;
  cursor: pointer;
}

.pick svg {
  width: 15px;
  height: 15px;
}

.collection.on .pick {
  color: #fff;
  background: var(--accent);
  border-color: var(--accent);
}

.videos {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 14px;
  margin-top: 14px;
}

.video {
  padding: 8px;
  border: 1px solid transparent;
  border-radius: var(--radius);
  cursor: pointer;
}

.video:hover {
  background: var(--hover);
}

.video.on {
  background: var(--accent-soft);
  border-color: var(--accent-line);
}

.cover {
  position: relative;
  aspect-ratio: 16 / 10;
  overflow: hidden;
  background: var(--thumb);
  border-radius: var(--radius-sm);
}

.cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.seq,
.len {
  position: absolute;
  padding: 1px 6px;
  font-size: 11px;
  color: #fff;
  background: rgba(20, 12, 16, 0.62);
  border-radius: 5px;
}

.seq {
  top: 6px;
  left: 6px;
}

.len {
  right: 6px;
  bottom: 6px;
}

.vtitle {
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow: hidden;
  margin: 8px 2px 0;
  font-size: 12.5px;
  font-weight: 600;
  line-height: 1.45;
}

.vrow {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin: 6px 2px 2px;
  font-size: 11.5px;
  color: var(--faint);
}

.owner {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.dl {
  flex: none;
  color: var(--accent-ink);
}

.foot,
.pager {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 14px;
}

.pager {
  padding-top: 12px;
  border-top: 1px solid var(--line-soft);
}

.count {
  font-size: 12.5px;
  color: var(--muted);
}

.pages {
  display: flex;
  align-items: center;
  gap: 5px;
}

.pages .ghost {
  padding: 4px 9px;
  font-size: 12.5px;
}

.page-now {
  min-width: 26px;
  padding: 4px 8px;
  font-size: 12.5px;
  text-align: center;
  color: #fff;
  background: var(--accent);
  border-radius: var(--radius-sm);
}

.jump {
  width: 58px;
  padding: 4px 7px;
  font: inherit;
  font-size: 12.5px;
  text-align: center;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.empty {
  display: flex;
  align-items: center;
  gap: 18px;
  margin-top: 16px;
  padding: 30px 26px;
  background: var(--raised);
  border: 1px dashed var(--line);
  border-radius: var(--radius);
}

.empty.tall {
  justify-content: center;
  min-height: 320px;
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
  margin: 6px 0 0;
  font-size: 12.5px;
  color: var(--muted);
  line-height: 1.7;
}
</style>
