<script setup>
/**
 * 图标渲染器——PNG 版。
 * 文件在 public/icons/<name>.png（白色 256px，无间距：内容占满整个画布）。
 * 尺寸完全由父元素的 CSS width/height 决定（与旧 SVG 行为一致——
 * 不需要任何 scale 补偿，object-fit: contain 恰好铺满）。
 * 颜色：深色主题白色原图直接显示；浅色主题 invert(1) 反转为深色。
 */
defineProps({ name: { type: String, required: true } });
</script>

<template>
  <img :src="'/icons/' + name + '.png'" :alt="''" class="icon-img" draggable="false" />
</template>

<style scoped>
.icon-img {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: contain;
  pointer-events: none;
  user-select: none;
  -webkit-user-drag: none;
}

/* 浅色主题：白色 PNG 反转为深色 */
:root[data-theme="light"] .icon-img {
  filter: invert(1);
}

@media (prefers-color-scheme: light) {
  :root:not([data-theme="dark"]) .icon-img {
    filter: invert(1);
  }
}
</style>
