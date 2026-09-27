<script setup>
import Icon from "../components/Icon.vue";
defineProps({
  version: { type: String, default: "" },
});
</script>

<template>
  <div>
    <section class="card">
      <div class="identity">
        <Icon name="download" class="mark" />
        <div>
          <h1>BILIdown</h1>
          <p class="ver num">v{{ version || "—" }}</p>
        </div>
      </div>

      <p class="desc">
        哔哩哔哩视频下载工具，支持普通投稿视频的 DASH 音视频分离下载与无损合成，
        扫码登录后可下载 1080P 及以上清晰度。
      </p>

      <dl class="rows">
        <div class="row">
          <dt>技术栈</dt>
          <dd>Tauri 2 · Rust · Vue 3 · SQLite</dd>
        </div>
        <div class="row">
          <dt>下载能力</dt>
          <dd>DASH 分片并发 · 区间断点写入 · ffmpeg 无损合成（-c copy）</dd>
        </div>
        <div class="row">
          <dt>清晰度</dt>
          <dd>登录最高 1080P；大会员可解锁 4K / HDR / 杜比视界 / 无损音轨</dd>
        </div>
        <div class="row">
          <dt>开发计划</dt>
          <dd>M3 批量下载、断点续传、弹幕字幕；M4 番剧课程、自动更新</dd>
        </div>
      </dl>

      <p class="disclaimer">
        仅供个人学习与离线观看使用，请遵守哔哩哔哩用户协议，不要用于传播或商业用途。
      </p>
    </section>
  </div>
</template>

<style scoped>
.card {
  padding: 22px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius-lg);
}

.identity {
  display: flex;
  align-items: center;
  gap: 13px;
}

.mark {
  width: 48px;
  height: 48px;
}

h1 {
  margin: 0;
  font-size: 22px;
  font-weight: 700;
  letter-spacing: -0.2px;
}

.ver {
  margin: 2px 0 0;
  font-size: 12.5px;
  color: var(--faint);
}

.desc {
  margin: 16px 0 0;
  font-size: 13px;
  line-height: 1.75;
  color: var(--muted);
  max-width: 62ch;
}

.rows {
  margin: 18px 0 0;
}

.row {
  display: flex;
  gap: 16px;
  padding: 12px 0;
  border-top: 1px solid var(--line-soft);
}

dt {
  flex: none;
  width: 86px;
  font-size: 12.5px;
  color: var(--muted);
}

dd {
  margin: 0;
  font-size: 13px;
}

.disclaimer {
  margin: 18px 0 0;
  padding: 11px 13px;
  font-size: 12px;
  line-height: 1.7;
  color: var(--muted);
  background: var(--raised);
  border-radius: var(--radius-sm);
}
</style>
