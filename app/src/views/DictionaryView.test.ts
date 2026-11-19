import { mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import { type IDictInfo, excludedDicts } from "../util/dicts";
import { lang } from "../util/settings";
import DictionaryView from "./DictionaryView.vue";

/** what the `dicts` command returns */
let dicts: Partial<IDictInfo>[] = [];

const dict = (name: string, lang: string | null, term_count = 10) => ({
  name,
  title: `${name} title`,
  lang,
  term_count,
});

/** the `req` of each `search` command */
let searches: Record<string, unknown>[] = [];

beforeEach(() => {
  searches = [];
  dicts = [];
  mockIPC((cmd, args) => {
    if (cmd === "search") {
      searches.push((args as { req: Record<string, unknown> }).req);
      return { items: [], next: null };
    }
    if (cmd === "segment") return [];
    if (cmd === "dicts") return dicts;
    throw new Error(`unexpected command: ${cmd}`);
  });
});

afterEach(() => {
  lang.value = "";
  excludedDicts.value = [];
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

  it("searches the words with a character read so", async () => {
    const wrapper = await mountView();
    await wrapper.get("input.search-input").setValue("*<興>{キョウ}");
    expect(await lastSearch()).toMatchObject({
      ch: "興",
      ch_reading: "キョウ",
    });
    expect(wrapper.get(".search-hint").text()).toContain(
      "Searching words with 興 read キョウ",
    );
    wrapper.unmount();
  });

  it("loads more results when collapsing leaves nothing to scroll", async () => {
    // a page of results, and more after it
    mockIPC((cmd, args) => {
      if (cmd !== "search") throw new Error(`unexpected command: ${cmd}`);
      const req = (args as { req: Record<string, unknown> }).req;
      searches.push(req);
      const offset = req.offset as number;
      const items = [0, 1, 2, 3, 4].map((i) => ({
        id: offset + i + 1,
        term: `語${offset + i}`,
        reading: "",
        entries: [],
      }));
      return { items, next: offset < 10 ? offset + 5 : null };
    });
    // jsdom has no layout: the list is as tall as its expanded results
    const clientHeight = vi
      .spyOn(HTMLElement.prototype, "clientHeight", "get")
      .mockReturnValue(500);
    const scrollHeight = vi
      .spyOn(HTMLElement.prototype, "scrollHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.querySelectorAll(".accordion-content").length * 1000;
      });

    const wrapper = await mountView();
    await wrapper.get("input.search-input").setValue("語");
    await lastSearch();
    await flushPromises();
    expect(searches).toHaveLength(1);
    expect(wrapper.findAll(".accordion-item")).toHaveLength(5);

    // collapse them all: nothing to scroll, so the next page loads
    for (const header of wrapper.findAll(".accordion-header")) {
      await header.trigger("click");
    }
    await vi.waitFor(() => expect(searches.length).toBeGreaterThan(1));
    expect(searches[1]).toMatchObject({ offset: 5 });

    clientHeight.mockRestore();
    scrollHeight.mockRestore();
    wrapper.unmount();
  });

  it("loads more results with the More button", async () => {
    mockIPC((cmd, args) => {
      if (cmd !== "search") throw new Error(`unexpected command: ${cmd}`);
      const req = (args as { req: Record<string, unknown> }).req;
      searches.push(req);
      const offset = req.offset as number;
      return {
        items: [
          { id: offset + 1, term: `語${offset}`, reading: "", entries: [] },
        ],
        next: offset < 5 ? offset + 5 : null,
      };
    });
    // tall enough not to load more by itself
    const clientHeight = vi
      .spyOn(HTMLElement.prototype, "clientHeight", "get")
      .mockReturnValue(500);
    const scrollHeight = vi
      .spyOn(HTMLElement.prototype, "scrollHeight", "get")
      .mockReturnValue(100000);

    const wrapper = await mountView();
    await wrapper.get("input.search-input").setValue("語");
    await lastSearch();
    await flushPromises();
    await wrapper.get(".more-button").trigger("click");
    await vi.waitFor(() => expect(searches).toHaveLength(2));
    expect(searches[1]).toMatchObject({ offset: 5 });
    await flushPromises();
    // no more after that
    expect(wrapper.find(".more-button").exists()).toBe(false);

    clientHeight.mockRestore();
    scrollHeight.mockRestore();
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
  it("marks terms with their language and definitions with the dictionary's", async () => {
    const glossary = JSON.stringify([
      {
        type: "structured-content",
        content: [
          { tag: "span", lang: "zh-Hant", content: "學習" },
          { tag: "span", lang: "zh-Hans", content: "学习" },
        ],
      },
    ]);
    mockIPC((cmd, args) => {
      if (cmd === "dict_styles") return null;
      if (cmd === "dicts") return [];
      if (cmd !== "search") throw new Error(`unexpected command: ${cmd}`);
      searches.push((args as { req: Record<string, unknown> }).req);
      const entry = {
        dict_id: 1,
        dict_path: "CC-CEDICT",
        lang: "zh",
        target_lang: "en",
        term_id: 1,
        term: "學習",
        reading: "xué xí",
        def_tags: "",
        rules: "",
        score: 0,
        glossary,
        sequence: null,
        tags: "",
      };
      return {
        items: [{ id: 1, term: "學習", reading: "", entries: [entry] }],
        next: null,
      };
    });
    const wrapper = await mountView();
    await wrapper.get("input.search-input").setValue("學習");
    await lastSearch();
    await flushPromises();
    expect(wrapper.get(".headword").attributes("lang")).toBe("zh-Hant");
    // the first result opens by itself
    // English definitions, but their Han characters are Chinese
    expect(wrapper.get(".gloss-list").attributes("lang")).toBe("zh");
    wrapper.unmount();
  });
  it("leaves out the dictionaries unticked, and remembers them", async () => {
    // listed without a language first: MyDict, then A
    dicts = [dict("A", "ja-JP"), dict("MyDict", null)];
    const wrapper = await mountView();
    await flushPromises();
    const boxes = wrapper.findAll(".dict-filter-list input");
    expect(boxes.map((b) => (b.element as HTMLInputElement).checked)).toEqual([
      true,
      true,
    ]);

    await boxes[0].setValue(false);
    await wrapper.get("input.search-input").setValue("日本");
    expect(await lastSearch()).toMatchObject({ exclude_dicts: ["MyDict"] });
    expect(wrapper.get(".dict-filter-count").text()).toBe("(1 of 2)");
    expect(JSON.parse(localStorage.getItem("cjk-dict.excludedDicts")!)).toEqual(
      ["MyDict"],
    );

    searches = [];
    await wrapper.get(".dict-filter .link-button").trigger("click");
    expect(await lastSearch()).toMatchObject({ exclude_dicts: [] });
    wrapper.unmount();
  });

  it("hides the dictionary filter with one dictionary", async () => {
    // a kanji dictionary has no words to choose
    dicts = [dict("A", "ja-JP"), dict("Kanji", "ja-JP", 0)];
    const wrapper = await mountView();
    await flushPromises();
    expect(wrapper.find(".dict-filter").exists()).toBe(false);
    wrapper.unmount();
  });
  it("lists the dictionaries of the search language only", async () => {
    lang.value = "zh";
    dicts = [
      dict("A", "ja-JP"),
      dict("B", "zh-TW"),
      dict("C", "zh"),
      dict("MyDict", null),
    ];
    excludedDicts.value = ["A", "B"];
    const wrapper = await mountView();
    await flushPromises();
    const names = wrapper
      .findAll(".dict-filter-list span")
      .map((s) => s.text());
    expect(names).toEqual(["MyDict title", "C title", "B title"]);
    expect(wrapper.get(".dict-filter-count").text()).toBe("(2 of 3)");

    // A, of another language, stays left out
    await wrapper.get(".dict-filter .link-button").trigger("click");
    expect(excludedDicts.value).toEqual(["A"]);
    wrapper.unmount();
  });

  it("hides the dictionary filter with one dictionary of the language", async () => {
    lang.value = "ko-KR";
    dicts = [dict("A", "ja-JP"), dict("B", "ko-KR")];
    const wrapper = await mountView();
    await flushPromises();
    expect(wrapper.find(".dict-filter").exists()).toBe(false);
    wrapper.unmount();
  });
  it("keeps the last dictionary ticked", async () => {
    dicts = [dict("A", "ja-JP"), dict("B", "ja-JP")];
    excludedDicts.value = ["A"];
    const wrapper = await mountView();
    await flushPromises();
    const boxes = wrapper.findAll(".dict-filter-list input");
    // A is left out and can be ticked; B is the last one ticked
    expect(boxes.map((b) => b.attributes("disabled") !== undefined)).toEqual([
      false,
      true,
    ]);
    // some are left out: the count shows even collapsed
    expect(wrapper.get(".dict-filter").classes()).not.toContain(
      "dict-filter-all",
    );
    wrapper.unmount();
  });

  it("warns when the search language's dictionaries are all left out", async () => {
    dicts = [dict("A", "ja-JP"), dict("B", "ko-KR")];
    excludedDicts.value = ["B"];
    const wrapper = await mountView();
    await flushPromises();
    expect(wrapper.find(".dict-filter-warning").exists()).toBe(false);
    expect(wrapper.get(".dict-filter").classes()).not.toContain(
      "dict-filter-all",
    );

    lang.value = "ko-KR";
    await flushPromises();
    expect(wrapper.find(".dict-filter-warning").exists()).toBe(true);
    // listed even alone, so that it can be ticked again; not ticked for the user
    const box = wrapper.get(".dict-filter-list input");
    expect((box.element as HTMLInputElement).checked).toBe(false);
    expect(wrapper.get(".dict-filter-count").text()).toBe("(0 of 1)");

    await box.setValue(true);
    expect(excludedDicts.value).toEqual([]);
    expect(wrapper.find(".dict-filter-warning").exists()).toBe(false);
    // one dictionary, ticked: nothing to choose
    expect(wrapper.find(".dict-filter").exists()).toBe(false);
    wrapper.unmount();
  });
});
