/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** cjk-dict worker URL for production builds (dev uses the Vite proxy at /api) */
  readonly VITE_DICT_API_URL?: string;
  readonly VITE_PRIMEUI_LICENSE_KEY?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<{}, {}, any>;
  export default component;
}
