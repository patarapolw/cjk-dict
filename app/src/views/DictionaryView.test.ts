import { mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import { lang } from "../util/settings";
import DictionaryView from "./DictionaryView.vue";

/** the `req` of each `search` command */
let searches: Record<string, unknown>[] = [];

beforeEach(() => {
  searches = [];
  mockIPC((cmd, args) => {
    if (cmd === "search") {
      searches.push((args as { req: Record<string, unknown> }).req);
      return { items: [], next: null };
    }
    if (cmd === "segment") return [];
    throw new Error(`unexpected command: ${cmd}`);
  });
});

afterEach(() => {
  lang.value = "";
});

async function mountView() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/dictionary", component: DictionaryView }],
  });
  await router.push("/dictionary");
  await router.isReady();
  return mount(DictionaryView, {
    global: { plugins: [router] },
    attachTo: document.body,
  });
}

const readingButton = (wrapper: Awaited<ReturnType<typeof mountView>>) =>
  wrapper.get('button[title^="Reading search"]');

/** the search sent once typing pauses */
async function lastSearch() {
  await vi.waitFor(() => expect(searches.length).toBeGreaterThan(0));
  await flushPromises();
  return searches[searches.length - 1];
}

describe("DictionaryView", () => {
  it("searches by reading with the {} toggle on", async () => {
    const wrapper = await mountView();
    await readingButton(wrapper).trigger("click");
    expect(readingButton(wrapper).attributes("aria-pressed")).toBe("true");

    await wrapper.get("input.search-input").setValue("にほんご");
    expect(await lastSearch()).toMatchObject({
      term: "",
      reading: "にほんご",
      reading_glob: "",
    });
    expect(wrapper.get(".search-hint").text()).toContain("by reading");
    wrapper.unmount();
  });

  it("searches the term without the {} toggle", async () => {
    const wrapper = await mountView();
    await wrapper.get("input.search-input").setValue("にほんご");
    expect(await lastSearch()).toMatchObject({
      term: "にほんご",
      reading: "",
    });
    wrapper.unmount();
  });

  it("lets furigana syntax typed by hand win over the {} toggle", async () => {
    const wrapper = await mountView();
    await readingButton(wrapper).trigger("click");
    await wrapper.get("input.search-input").setValue("<日本>{にほん}語");
    expect(await lastSearch()).toMatchObject({
      term: "日本語",
      reading: "",
      reading_glob: "にほん*",
    });
    wrapper.unmount();
  });

  it("searches Korean by Hangul reading with the {} toggle", async () => {
    lang.value = "ko-KR";
    const wrapper = await mountView();
    await readingButton(wrapper).trigger("click");
    await wrapper.get("input.search-input").setValue("광속");
    expect(await lastSearch()).toMatchObject({
      lang: "ko-KR",
      term: "",
      reading: "광속",
    });
    wrapper.unmount();
  });
});
