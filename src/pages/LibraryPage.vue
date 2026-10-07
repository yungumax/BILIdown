<script setup>
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { animate, stagger } from "animejs";
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
/** 每页多少条（对应参考图里的「每批」）：改了立刻回第 1 页重取 */
const pageSize = ref(20);
const PAGE_SIZES = [20, 50, 100, 200];
const pickingParse = ref(false);
const parsing = ref(false);

const loggedIn = computed(() => !!props.login?.logged_in);

const folders = computed(() => {
  const list = tab.value === TAB.fav ? account.value?.created ?? [] : account.value?.subscribed ?? [];
  const keyword = query.value.trim().toLowerCase();
  if (!keyword) return list;
  return list.filter((item) =>
    [item.title, item.owner, item.intro].some((text) => (text ?? "").toLowerCase().includes(keyword))
  );
});

/** 两个标签合起来：勾选可以跨标签，取来源时不能只看当前标签那一列 */
const allFolders = computed(() => [
  ...(account.value?.created ?? []),
  ...(account.value?.subscribed ?? []),
]);

/** 勾选用的键：收藏夹和合集的 id 是两套命名空间，可能撞号，得带上 kind */
const keyOf = (folder) => `${folder.kind === "season" ? "season" : "fav"}:${folder.id}`;

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

/** 集合链接：按 kind 分流。
 *
 *  - 合集（订阅来的都是这一类）→ `channel/collectiondetail?sid=`，走 seasons_archives_list；
 *  - 收藏夹 → `favlist?fid=`，订阅来的要用**对方**的 mid。
 *
 *  拿合集的 id 去查 favlist 不报错，但会命中另一个用户的收藏夹、返回 0 条，
 *  界面上就表现为「没有可解析的内容」——所以不能一律用 favlist。 */
const favUrl = (folder) => {
  const mid = folder.owner_mid || account.value?.mid || 0;
  return folder.kind === "season"
    ? `https://space.bilibili.com/${mid}/channel/collectiondetail?sid=${folder.id}`
    : `https://space.bilibili.com/${mid}/favlist?fid=${folder.id}`;
};

function switchTab(next) {
  if (tab.value === next) return;
  tab.value = next;
  opened.value = null; // 切标签退回一级：不然停在详情里，看着像"点了没反应"
  query.value = "";
}

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

function togglePick(key) {
  const next = new Set(picked.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  picked.value = next;
}

/** 打开集合：每页多少条由接口决定（收藏夹 20），翻页就是按起点再探一次 */
async function openFolder(folder) {
  opened.value = { folder, probe: null, items: [], page: 1, total: 0, picked: new Set(), loading: true };
  await loadPage(1);
}

async function loadPage(page) {
  const view = opened.value;
  if (!view) return;
  view.loading = true;
  try {
    const perPage = pageSize.value;
    const probe = await api.probeRange(
      favUrl(view.folder),
      (page - 1) * perPage + 1,
      perPage
    );
    view.probe = probe;
    view.items = probe.items ?? [];
    view.total = probe.total || view.items.length;
    view.page = page;
    view.picked = new Set();
    // 翻页：新一页的封面格子级联亮起
    if (!matchMedia("(prefers-reduced-motion: reduce)").matches) {
      await nextTick();
      requestAnimationFrame(() => {
        const cells = [...document.querySelectorAll(".videos .video")].slice(0, 24);
        if (cells.length) animate(cells, { opacity: [0, 1], translateY: [8, 0], duration: 300, delay: stagger(22), ease: "outExpo" });
      });
    }
  } catch (error) {
    emit("toast", String(error));
  } finally {
    view.loading = false;
  }
}

async function changePageSize(size) {
  pageSize.value = Number(size) || 20;
  if (opened.value) await loadPage(1);
}

const pageCount = computed(() => {
  const view = opened.value;
  if (!view) return 1;
  return Math.max(1, Math.ceil(view.total / Math.max(1, pageSize.value)));
});

/** 页码按钮：当前页附近最多 7 个，两头自动收窄 */
const pageList = computed(() => {
  const total = pageCount.value;
  const current = opened.value?.page ?? 1;
  const window = 3;
  let start = Math.max(1, current - window);
  let end = Math.min(total, start + 6);
  start = Math.max(1, end - 6);
  const pages = [];
  for (let p = start; p <= end; p += 1) pages.push(p);
  return pages;
});

/** 解析全部：把整个集合逐页拉进来（供勾选下载） */
async function loadAll() {
  const view = opened.value;
  if (!view || parsing.value) return;
  parsing.value = true;
  pickingParse.value = false;
  const pages = pageCount.value;
  try {
    for (let page = 1; page <= pages; page += 1) {
      const probe = await api.probeRange(favUrl(view.folder), (page - 1) * pageSize.value + 1, pageSize.value);
      const items = probe.items ?? [];
      if (items.length) view.items = page === 1 ? items : [...view.items, ...items];
      view.total = probe.total || view.total;
      emit("toast", `已加载 ${view.items.length} / ${view.total} 项`);
      if (!items.length) break;
    }
  } catch (error) {
    emit("toast", String(error));
  } finally {
    parsing.value = false;
  }
}

/** 后台解析全部并下载：逐页取元数据、直接入队（不占列表） */
async function parseAllAndDownload() {
  const view = opened.value;
  if (!view || parsing.value) return;
  parsing.value = true;
  pickingParse.value = false;
  const pages = pageCount.value;
  let started = 0;
  try {
    for (let page = 1; page <= pages; page += 1) {
      const from = (page - 1) * pageSize.value + 1;
      const probe = await api.probeRange(favUrl(view.folder), from, pageSize.value);
      const items = probe.items ?? [];
      if (!items.length) break;
      started += await enqueueBatch(probe, items, {
        quality: pickDefaultQuality(probe, props.settings),
        audio: pickDefaultAudio(probe, props.settings),
        absoluteOf: (index) => Math.max((probe.total || view.total) - (from + index) + 1, 1),
        onError: (entry, error) => emit("toast", `${entry.title}: ${error}`),
      });
    }
    emit("toast", `已加入 ${started} 个下载任务`);
  } catch (error) {
    emit("toast", String(error));
  } finally {
    parsing.value = false;
  }
}

/** 这一页第 index 条在来源里的序号（1 = 最旧），与解析页同一套算法 */
const absoluteOf = (index) => {
  const view = opened.value;
  if (!view) return index + 1;
  const position = (view.page - 1) * pageSize.value + index;
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

/** 选中若干个集合（可以跨标签）→ 交给解析页按各自的链接解析（那边继续筛、还能改清晰度） */
function openPicked() {
  const list = allFolders.value.filter((item) => picked.value.has(keyOf(item)));
  if (!list.length) return;
  emit("open-source", list.map((item) => favUrl(item)).join("\n"));
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

/** 详情页"已选 N"计数滚动：勾选变化时补间而不是跳变 */
const shownPicked = ref(0);
watch(
  () => opened.value?.picked.size ?? 0,
  (to) => {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches || to === shownPicked.value) {
      shownPicked.value = to;
      return;
    }
    const obj = { v: shownPicked.value };
    animate(obj, {
      v: to,
      duration: 360,
      ease: "out(3)",
      onUpdate: () => {
        shownPicked.value = Math.round(obj.v);
      },
    });
  },
  { immediate: true }
);

/** 集合卡 3D 微倾斜（±2.4°）：指针在哪边卡片就朝哪边轻轻转，移开复位。
    倾斜写在卡片自己的 transform 上（含悬停那 1px 上浮），纯动效零视觉。 */
let tiltedCard = null;

function onTilt(event) {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const card = event.target instanceof Element ? event.target.closest(".collection") : null;
  if (!card) return;
  if (tiltedCard && tiltedCard !== card) tiltedCard.style.transform = "";
  tiltedCard = card;
  const r = card.getBoundingClientRect();
  const px = (event.clientX - r.left) / r.width - 0.5;
  const py = (event.clientY - r.top) / r.height - 0.5;
  card.style.transform = `translateY(-1px) rotateX(${(-py * 2.4).toFixed(2)}deg) rotateY(${(px * 2.4).toFixed(2)}deg)`;
}
function offTilt() {
  // pointerleave 绑在容器上，target 是容器不是卡——按记录复位
  if (tiltedCard) {
    tiltedCard.style.transform = "";
    tiltedCard = null;
  }
}

// 入场：集合卡首次渲染出来后逐张浮起（列表以列表的方式出现；
// 一次性编排，减少动态下不演）
let roseIn = false;
watch(
  () => folders.value.length,
  async (n) => {
    if (!n || roseIn) return;
    roseIn = true;
    await nextTick();
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    animate(".card .collection", {
      opacity: [0, 1],
      translateY: [16, 0],
      duration: 520,
      delay: stagger(60),
      ease: "outExpo",
    });
  },
);
</script>

<template>
  <div class="lib-page">
    <section class="card" :class="{ 'lib-fill': !opened && folders.length }">
      <div class="tabs">
        <button :class="{ active: tab === TAB.fav }" @click="switchTab(TAB.fav)">
          收藏夹
          <span v-if="account" class="badge">{{ account.created.length }}</span>
        </button>
        <button :class="{ active: tab === TAB.sub }" @click="switchTab(TAB.sub)">
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
          <span class="grow"></span>
          <button class="ghost" :disabled="loading" title="刷新" @click="load">
            <Icon name="refresh" class="btn-icon" :class="{ spin: loading }" />
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

        <div v-else-if="folders.length" class="cards page-in" @pointermove="onTilt" @pointerleave="offTilt">
          <article
            v-for="item in folders"
            :key="keyOf(item)"
            class="collection"
            :class="{ on: picked.has(keyOf(item)) }"
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
              <p class="sub">
                {{
                  item.kind === "season"
                    ? `合集 · ${item.owner}`
                    : item.owner
                      ? `收藏夹 · ${item.owner}`
                      : "我创建的收藏夹"
                }}
              </p>
              <p v-if="item.intro" class="intro">{{ item.intro }}</p>
              <p class="num">{{ item.media_count }} 个视频</p>
            </div>
            <button
              class="pick"
              :title="picked.has(keyOf(item)) ? '取消选择' : '选择这个集合'"
              @click.stop="togglePick(keyOf(item))"
            >
              <Icon :name="picked.has(keyOf(item)) ? 'check' : 'plus'" />
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
          <span class="loaded">已加载 <b>{{ opened.items.length }}</b> / {{ opened.total }} 项</span>
          <select
            class="page-size"
            :value="pageSize"
            title="每页加载多少条"
            @change="changePageSize($event.target.value)"
          >
            <option v-for="size in PAGE_SIZES" :key="size" :value="size">每批 {{ size }}</option>
          </select>
          <div class="parse-split">
            <button class="seg main" :disabled="parsing" @click="loadAll">
              {{ parsing ? "解析中…" : "解析全部" }}
            </button>
            <button class="seg arrow" :class="{ on: pickingParse }" title="更多解析方式" @click="pickingParse = !pickingParse">
              <Icon name="chevronDown" />
            </button>
            <div v-if="pickingParse" class="parse-pop">
              <button class="parse-item" @click="loadAll">
                <Icon name="listDetails" />
                解析全部
              </button>
              <button class="parse-item" @click="parseAllAndDownload">
                <Icon name="cloudDownload" />
                后台解析全部并下载
              </button>
            </div>
          </div>
          <button class="ghost" :disabled="!opened.items.length" @click="downloadPicked(true)">下载全部</button>
          <button class="primary" :disabled="!opened.picked.size" @click="downloadPicked(false)">
            下载所选（{{ opened.picked.size }}）
          </button>
        </header>

        <p v-if="opened.loading" class="hint pad">读取中…</p>
        <div v-else-if="opened.items.length" class="videos page-in">
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
          <span class="count">已选 {{ shownPicked }} / {{ opened.total }}</span>
          <div class="pages">
            <button class="ghost" :disabled="opened.page <= 1" @click="loadPage(1)">«</button>
            <button class="ghost" :disabled="opened.page <= 1" @click="loadPage(opened.page - 1)">‹</button>
            <button
              v-for="page in pageList"
              :key="page"
              class="ghost page-btn"
              :class="{ on: page === opened.page }"
              @click="loadPage(page)"
            >
              {{ page }}
            </button>
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
            <span class="jump-label">跳转</span>
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

.search input {
  width: 100%;
  padding: 9px 12px 9px 34px;
  font: inherit;
  font-size: 13px;
  color: var(--text);
  transition: border-color var(--motion-fast) var(--ease-out);
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
  perspective: 640px;
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  /* 卡片之间的间隔略微放大，透气一点（用户点名） */
  gap: 18px;
  margin-top: 16px;
}

.collection {
  position: relative;
  display: flex;
  gap: 13px;
  padding: 12px;
  background: var(--card);
  /* 悬停反馈：边框变色 + 1px 上浮，平面语言里的一点"可点" */
  transition:
    background var(--motion-fast) var(--ease-out),
    border-color var(--motion-fast) var(--ease-out),
    transform var(--motion-fast) var(--ease-out);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  cursor: pointer;
}

.collection:hover {
  border-color: var(--accent-line);
  transform: translateY(-1px);
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
  /* 封面紧跟工具条的分隔线，中间不留空隙 */
  margin-top: 0;
}

.video {
  padding: 8px;
  border: 1px solid transparent;
  border-radius: var(--radius);
  cursor: pointer;
  transition:
    background var(--motion-fast) var(--ease-out),
    border-color var(--motion-fast) var(--ease-out);
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
  /* 悬停时封面轻微放大（装饰性微交互，容器已裁圆角） */
  transition: transform var(--motion) var(--ease-out);
}

.video:hover .cover img,
.video.on .cover img {
  transform: scale(1.04);
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

/* 页码栏：钉在窗口最底，滚视频卡片时它不走开（用户点名要固定、且要封住底边）。
   sticky 以 .content 的内容盒为基准，而它还有 22px 底 padding（App.vue），
   bottom:0 会悬在窗口底上方 22px，那条缝里封面会从页码栏底下滚出来 ——
   所以补 -22px 让整个盒子封到窗口底边。 */
.pager {
  position: sticky;
  bottom: -22px;
  z-index: 2;
  padding: 12px 0 10px;
  background: var(--card);
  border-top: 1px solid var(--line-soft);
  /* 卡片是从下面滚上来的，圆角与两侧用同色描边圈盖住 */
  box-shadow: 0 0 0 12px var(--card);
}

/* 集合列表的底栏：硬固定在卡片底部（不随滚轮，滚到底也不释放）。
   机制：卡片撑满可用高度、列表改为卡内滚动，底栏是布局的最后一段。
   高度用 **flex 链**（.content 见 App.vue 的 :has 规则 → .lib-page → 本卡片
   flex:1），不用 100vh——应用有随窗口缩放的整体 UI zoom（小窗口最低 0.85），
   vh 单位按未缩放视口计算、渲染再乘 zoom，小窗口下会矮掉一整截（底栏悬空）。
   flex 各级都在同一缩放坐标系里，天然一致。
   -22px 底边距把卡片底边封到窗口底边（抵消 .content 的 22px 下内边距）。 */
.lib-page {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.card.lib-fill {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  margin-bottom: -22px;
  padding-bottom: 0;
}

.card.lib-fill .cards {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  /* 网格默认 align-content:stretch 会把行拉高填满剩余空间——卡片被拉成
     一整条长盒子。行保持自然高度、顶部堆叠，空出来的留在下方 */
  align-content: start;
}

.card.lib-fill .foot {
  flex: none;
  margin-top: 0;
  padding: 12px 0 10px;
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

/* 「每批 N」下拉与「解析」分段按钮：跟解析页工具条同一套语言 */
.page-size {
  flex: none;
  padding: 6px 9px;
  font: inherit;
  font-size: 12.5px;
  color: var(--text);
  background: var(--field);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.parse-split {
  position: relative;
  flex: none;
  display: flex;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  overflow: visible;
}

.parse-split .seg {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 6px 10px;
  font-size: 12.5px;
  color: var(--text);
  background: var(--field);
  border: none;
  cursor: pointer;
}

.parse-split .seg.main {
  border-right: 1px solid var(--line);
}

.parse-split .seg.arrow {
  padding: 6px 8px;
  color: var(--muted);
}

.parse-split .seg.arrow svg {
  width: 14px;
  height: 14px;
}

.parse-split .seg.arrow.on {
  color: var(--accent-ink);
}

.parse-split .seg:hover:not(:disabled) {
  background: var(--raised);
}

.parse-pop {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 12;
  min-width: 208px;
  padding: 5px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  box-shadow: 0 12px 30px rgba(20, 12, 16, 0.24);
}

.parse-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 9px;
  font-size: 12.5px;
  text-align: left;
  color: var(--text);
  background: none;
  border: none;
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.parse-item:hover {
  background: var(--hover);
}

.parse-item svg {
  width: 16px;
  height: 16px;
  color: var(--muted);
}

.page-btn.on {
  /* 换页时当前页码弹一下（active 类换到哪个钮哪个钮演） */
  animation: page-on-pop 260ms var(--ease-out-expo);
}

@keyframes page-on-pop {
  0% {
    transform: scale(0.86);
  }
  60% {
    transform: scale(1.08);
  }
  100% {
    transform: none;
  }
}

.page-btn {
  min-width: 30px;
  text-align: center;
}

.page-btn.on {
  color: #fff;
  background: var(--accent);
  border-color: var(--accent);
}

.jump-label {
  font-size: 12.5px;
  color: var(--muted);
}

.pages .ghost {
  padding: 4px 9px;
  font-size: 12.5px;
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
  /* 空状态的图标轻轻浮着——页面没内容时也有一点生命感 */
  animation: float-y 3.2s ease-in-out infinite;
}

@keyframes float-y {
  50% {
    transform: translateY(-4px);
  }
}

.empty-text {
  flex: 1;
  min-width: 0;
}

.empty-text .title {
  margin: 0;
  font-size: 14px;
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

<style scoped>
/* 第六轮追加：勾选角标（+ → ✓）切换时轻弹 */
.collection.on .pick {
  animation: pick-pop 260ms var(--ease-out-expo);
}

@keyframes pick-pop {
  0% { transform: scale(0.8) rotate(-6deg); opacity: 0.4; }
  60% { transform: scale(1.08) rotate(2deg); opacity: 1; }
  100% { transform: none; opacity: 1; }
}
</style>

<style scoped>
/* 第七轮追加：空状态图标轻浮动 */
.empty-icon {
  animation: lib-float 3.2s ease-in-out infinite;
}

@keyframes lib-float {
  50% { transform: translateY(-4px); }
}
</style>
