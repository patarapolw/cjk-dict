import { defineConfig, mergeConfig } from "vitest/config";

import viteConfig from "./vite.config.ts";

// https://tauri.app/develop/tests/mocking/
export default defineConfig((env) =>
  mergeConfig(viteConfig(env), {
    test: {
      environment: "jsdom",
      include: ["src/**/*.test.ts"],
      setupFiles: ["test/setup.ts"],
    },
  }),
);
