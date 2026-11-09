// @ts-expect-error type error without @types/node package
import process from "node:process";
import { defineConfig, loadEnv } from "vite";

import vue from "@vitejs/plugin-vue";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(({ command, mode }) => {
  const env = loadEnv(mode, process.cwd(), "VITE_");

  // Production talks to the deployed worker directly; fail early instead of shipping
  // an app with no server.
  if (command === "build" && !env.VITE_DICT_API_URL) {
    throw new Error(
      "VITE_DICT_API_URL is not set. Set it (e.g. in app/.env.production) to the deployed " +
        "cjk-dict worker, e.g. https://cjk-dict.<account>.workers.dev",
    );
  }

  return {
    plugins: [vue()],

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    //
    // 1. prevent Vite from obscuring rust errors
    clearScreen: false,
    // 2. tauri expects a fixed port, fail if that port is not available
    server: {
      port: 1420,
      strictPort: true,
      host: host || false,
      hmr: host
        ? {
            protocol: "ws",
            host,
            port: 1421,
          }
        : undefined,
      watch: {
        // 3. tell Vite to ignore watching `src-tauri`
        ignored: ["**/src-tauri/**"],
      },
      // 4. dev: the app calls same-origin /api/*, forwarded to `wrangler dev` (the worker).
      //    Also works for `tauri android dev`, since the device reaches this server.
      //    WRANGLER_DEV_URL points it elsewhere, e.g. a deployed worker.
      proxy: {
        "/api": {
          target: process.env.WRANGLER_DEV_URL || "http://localhost:8787",
          changeOrigin: true,
          rewrite: (path: string) => path.replace(/^\/api/, ""),
        },
      },
    },
  };
});
