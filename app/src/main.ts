import "./index.css";

import PrimeVue from "primevue/config";
import Tooltip from "primevue/tooltip";
import { createApp } from "vue";

import Aura from "@primeuix/themes/aura";

import App from "./App.vue";
import { router } from "./router.ts";
import { settingsState } from "./util/settings.ts";

settingsState.load();

createApp(App)
  .use(PrimeVue, {
    theme: {
      preset: Aura,
    },
    license: import.meta.env.VITE_PRIMEUI_LICENSE_KEY,
  })
  .directive("tooltip", Tooltip)
  .use(router)
  .mount("#app");
