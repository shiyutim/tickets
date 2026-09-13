import { createApp } from "vue";
import "@arco-design/web-vue/es/message/style/css.js";
import "@arco-design/web-vue/es/notification/style/css.js";
import "./styles.css";
import App from "./App.vue";
import router from "./router";
import { applyTheme, readSavedTheme } from "./services/theme";
applyTheme(readSavedTheme());
createApp(App).use(router).mount("#app");
