import "./index.css";
import "./components/yomitan/glossary.css";

import { createApp } from "vue";

import App from "./App.vue";
import { router } from "./router.ts";

createApp(App).use(router).mount("#app");
