// @ts-expect-error type error without @types/node package
import { randomFillSync } from "node:crypto";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, beforeAll } from "vitest";

// Tauri's IPC uses WebCrypto, which jsdom doesn't provide
// (https://tauri.app/develop/tests/mocking/)
beforeAll(() => {
  Object.defineProperty(window, "crypto", {
    configurable: true,
    value: {
      getRandomValues: (buffer: ArrayBufferView) => randomFillSync(buffer),
    },
  });
});

afterEach(() => {
  clearMocks();
});
