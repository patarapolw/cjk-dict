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

  it("doesn't retry a failing search, but searches again on the next query", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => {});
    // jsdom has no layout: give the result list a height, so an empty one leaves
    // room to "scroll" for more, as on a device
    const clientHeight = vi
      .spyOn(HTMLElement.prototype, "clientHeight", "get")
      .mockReturnValue(500);
    mockIPC((cmd, args) => {
      if (cmd === "search") {
        searches.push((args as { req: Record<string, unknown> }).req);
        // end a retry loop, so the assertion below fails instead of the worker
        if (searches.length > 10) return { items: [], next: null };
        throw new Error("no dictionaries downloaded yet");
      }
      throw new Error(`unexpected command: ${cmd}`);
    });

    const wrapper = await mountView();
    await wrapper.get("input.search-input").setValue("日本");
    await lastSearch();
    // give a retry loop the time to show itself
    await new Promise((r) => setTimeout(r, 100));
    await flushPromises();
    expect(searches).toHaveLength(1);

    await wrapper.get("input.search-input").setValue("日本語");
    await vi.waitFor(() => expect(searches).toHaveLength(2));
    expect(searches[1]).toMatchObject({ term: "日本語" });

    consoleError.mockRestore();
    clientHeight.mockRestore();
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
