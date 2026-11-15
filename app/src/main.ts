import "./index.css";

import PrimeVue from "primevue/config";
import Tooltip from "primevue/tooltip";
import { createApp } from "vue";

import Aura from "@primeuix/themes/aura";

import App from "./App.vue";
import { router } from "./router.ts";

createApp(App)
  .use(PrimeVue, {
    theme: {
      preset: Aura,
    },
  })
  .directive("tooltip", Tooltip)
  .use(router)
  .mount("#app");
