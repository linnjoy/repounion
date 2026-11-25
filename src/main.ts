import { createApp } from "vue";
import App from "./App.vue";
import "./styles.css";
import { initTheme } from "./theme";

// 先确定主题再挂载，避免启动时闪一下默认配色。
initTheme();
createApp(App).mount("#app");
