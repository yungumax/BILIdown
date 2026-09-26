import { createApp } from "vue";
import App from "./App.vue";
import "./styles.css";

// 窗口以隐藏方式启动；挂载完成后再显示，避免首帧白闪
createApp(App).mount("#app");

import("@tauri-apps/api/window")
  .then(({ getCurrentWindow }) => getCurrentWindow().show())
  .catch(() => {
    // 浏览器预览环境没有 Tauri 运行时，忽略
  });
