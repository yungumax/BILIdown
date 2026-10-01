<script setup>
/**
 * 图标渲染器——PNG 版。
 * 图标文件在 public/icons/<name>.png（白色 256px，来自用户提供的图标库）。
 * 颜色用 CSS filter 控制：深色主题下白色原图直接显示；浅色主题下
 * invert(1) 反转为黑色。激活态由父级 CSS 的 opacity/filter 变体实现。
 */
const props = defineProps({ name: { type: String, required: true } });
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
  /* PNG 有透明边距（256px 画布内容约 60-70%），视觉上比旧 SVG 小一圈；
     放大 30% 并保持居中，补偿边距让视觉尺寸对齐旧 SVG */
  transform: scale(1.3);
}

/* 浅色主题：白色 PNG 反转为深色 */
:root[data-theme="light"] .icon-img {
  filter: invert(1) brightness(0.8);
}

/* 跟随系统且系统是浅色时 */
@media (prefers-color-scheme: light) {
  :root:not([data-theme="dark"]) .icon-img {
    filter: invert(1) brightness(0.8);
  }
}
</style>
