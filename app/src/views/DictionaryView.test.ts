import { mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import { clipboardMonitorOn, lastClipboardText } from "../util/clipboard";
import { type IDictInfo, excludedDicts } from "../util/dicts";
import { lang } from "../util/settings";
import DictionaryView from "./DictionaryView.vue";
import { describedText, touchScreen } from "../util/tooltip";

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
  touchScreen.value = false;
  clipboardMonitorOn.value = false;
  lastClipboardText.value = "";
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
  wrapper
    .findAll("button")
    .find((b) => describedText(b.element).startsWith("Reading search"))!;

/** the search sent once typing pauses, as typed: before any search the other way */
async function firstSearch() {
  await vi.waitFor(() => expect(searches.length).toBeGreaterThan(0));
  await flushPromises();
  return searches[0];
}

/** the search sent once typing pauses */
async function lastSearch() {
  await vi.waitFor(() => expect(searches.length).toBeGreaterThan(0));
  await flushPromises();
  return searches[searches.length - 1];
}

describe("DictionaryView", () => {
  it("says on a touchscreen, before anything is typed, that a long press explains", async () => {
    const help = (w: Awaited<ReturnType<typeof mountView>>) =>
      w.find(".search-hint .touch-help").exists();
    const wrapper = await mountView();
    // with a mouse, hovering shows it
    expect(help(wrapper)).toBe(false);

    expect(wrapper.get(".search-hint").text()).toContain("or press {}");

    touchScreen.value = true;
    await flushPromises();
    expect(wrapper.get(".search-hint .touch-help").text()).toBe(
      "Long-press a button or option to see what it does.",
    );
    // which explains {} too, so the hint leaves it out, to save room
    expect(wrapper.get(".search-hint").text()).not.toContain("or press {}");
    await wrapper.get("input.search-input").setValue("日本");
    expect(help(wrapper)).toBe(false);
    wrapper.unmount();
  });

  it("searches by reading with the {} toggle on", async () => {
    const wrapper = await mountView();
    await readingButton(wrapper).trigger("click");
    expect(readingButton(wrapper).attributes("aria-pressed")).toBe("true");

    await wrapper.get("input.search-input").setValue("にほんご");
    expect(await firstSearch()).toMatchObject({
      term: "",
      reading: "にほんご",
      reading_glob: "",
    });
    expect(wrapper.get(".search-hint").text()).toContain("by reading");
    // nothing found by reading: then as a word
    await vi.waitFor(() => expect(searches).toHaveLength(2));
    expect(searches[1]).toMatchObject({ term: "にほんご", reading: "" });
    wrapper.unmount();
  });

  it("searches the term without the {} toggle", async () => {
    const wrapper = await mountView();
    await wrapper.get("input.search-input").setValue("にほんご");
    expect(await firstSearch()).toMatchObject({
      term: "にほんご",
      reading: "",
    });
    // nothing found as a word: then by reading
    await vi.waitFor(() => expect(searches).toHaveLength(2));
    expect(searches[1]).toMatchObject({ term: "", reading: "にほんご" });
    wrapper.unmount();
  });

  it.each([
    ["Token search", "~"],
    ["Exact match", "="],
  ])(
    "searches copied text in the normal mode, not by reading or %s",
    async (mode) => {
      mockIPC((cmd, args) => {
        if (cmd === "search") {
          searches.push((args as { req: Record<string, unknown> }).req);
          return { items: [], next: null };
        }
        if (cmd === "read_clipboard_text") return "日本語";
        if (cmd === "segment") return [];
        if (cmd === "dicts") return dicts;
        throw new Error(`unexpected command: ${cmd}`);
      });
      const wrapper = await mountView();
      const modeButton = wrapper
        .findAll("button")
        .find((b) => describedText(b.element).startsWith(mode))!;
      await modeButton.trigger("click");
      await readingButton(wrapper).trigger("click");
      expect(modeButton.attributes("aria-pressed")).toBe("true");

      await wrapper.get("input[name=clipboard]").setValue(true);
      await vi.waitFor(
        () =>
          expect(
            (wrapper.get("input.search-input").element as HTMLInputElement)
              .value,
          ).toBe("日本語"),
        { timeout: 3000 },
      );
      expect(modeButton.attributes("aria-pressed")).toBe("false");
      expect(readingButton(wrapper).attributes("aria-pressed")).toBe("false");
      expect(await firstSearch()).toMatchObject({
        term: "日本語",
        reading: "",
        type: null,
      });
      await wrapper.get("input[name=clipboard]").setValue(false);
      wrapper.unmount();
    },
  );

  it("keeps the clipboard monitor on after the view is left and shown again", async () => {
    let clip = "日本語";
    mockIPC((cmd, args) => {
      if (cmd === "search") {
        searches.push((args as { req: Record<string, unknown> }).req);
        return { items: [], next: null };
      }
      if (cmd === "read_clipboard_text") return clip;
      if (cmd === "segment") return [];
      if (cmd === "dicts") return dicts;
      throw new Error(`unexpected command: ${cmd}`);
    });
    const searchInput = (w: Awaited<ReturnType<typeof mountView>>) =>
      w.get("input.search-input").element as HTMLInputElement;

    let wrapper = await mountView();
    await wrapper.get("input[name=clipboard]").setValue(true);
    await vi.waitFor(() => expect(searchInput(wrapper).value).toBe("日本語"), {
      timeout: 3000,
    });
    wrapper.unmount(); // e.g. to the Characters view

    wrapper = await mountView();
    const box = wrapper.get("input[name=clipboard]")
      .element as HTMLInputElement;
    expect(box.checked).toBe(true);
    // the clip already searched isn't searched again over the view's own search
    await wrapper.get("input.search-input").setValue("中国");
    await new Promise((r) => setTimeout(r, 1500));
    expect(searchInput(wrapper).value).toBe("中国");
    // but a new clip is
    clip = "한국어";
    await vi.waitFor(() => expect(searchInput(wrapper).value).toBe("한국어"), {
      timeout: 3000,
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
    expect(await firstSearch()).toMatchObject({
      lang: "ko-KR",
      term: "",
      reading: "광속",
    });
    wrapper.unmount();
  });
  describe("searching the other way when nothing matches", () => {
    /** the `search` command finds a word for `found(req)` only */
    function findWhen(
      found: (req: Record<string, unknown>) => boolean,
      segment: string[] = [],
    ) {
      mockIPC((cmd, args) => {
        if (cmd === "search") {
          const req = (args as { req: Record<string, unknown> }).req;
          searches.push(req);
          const items = found(req)
            ? [{ id: 1, term: "中国", reading: " ZHONG1 GUO2 ", entries: [] }]
            : [];
          return { items, next: null };
        }
        if (cmd === "segment") return segment.map((w) => [w, []]);
        if (cmd === "dicts") return dicts;
        throw new Error(`unexpected command: ${cmd}`);
      });
    }
    const hint = (w: Awaited<ReturnType<typeof mountView>>) =>
      w.get(".search-hint").text().replace(/\s+/g, " ");

    // with Japanese, auto-convert to kana turns it into kana; with Korean, Latin letters
    // are romanization, a reading already
    it.each(["", "zh"])(
      "searches a word that isn't found by reading, before splitting it (%s)",
      async (l) => {
        lang.value = l;
        findWhen((req) => req.reading === "zhongguo");
        const wrapper = await mountView();
        await wrapper.get("input.search-input").setValue("zhongguo");
        await vi.waitFor(() =>
          expect(wrapper.findAll(".accordion-item")).toHaveLength(1),
        );
        expect(searches[0]).toMatchObject({ term: "zhongguo", reading: "" });
        expect(searches[1]).toMatchObject({ term: "", reading: "zhongguo" });
        expect(hint(wrapper)).toContain(
          "Nothing as a word: searching zhongguo by reading",
        );
        wrapper.unmount();
      },
    );

    it("splits a text into words when neither way finds it", async () => {
      findWhen(() => false, ["にほんご", "です"]);
      const wrapper = await mountView();
      await wrapper.get("input.search-input").setValue("にほんごです");
      await vi.waitFor(() =>
        expect(
          (wrapper.get("input.search-input").element as HTMLInputElement).value,
        ).toBe("にほんご です"),
      );
      expect(searches.slice(0, 2)).toMatchObject([
        { term: "にほんごです", reading: "" },
        { term: "", reading: "にほんごです" },
      ]);
      wrapper.unmount();
    });

    it("searches a reading that isn't found as a word", async () => {
      findWhen((req) => req.term === "sake");
      const wrapper = await mountView();
      await readingButton(wrapper).trigger("click");
      await wrapper.get("input.search-input").setValue("sake");
      await vi.waitFor(() =>
        expect(wrapper.findAll(".accordion-item")).toHaveLength(1),
      );
      expect(searches.slice(0, 2)).toMatchObject([
        { term: "", reading: "sake" },
        { term: "sake", reading: "" },
      ]);
      expect(hint(wrapper)).toContain(
        "Nothing by reading: searching sake as a word",
      );
      wrapper.unmount();
    });

    it("searches a reading with Han characters as a word, after as a reading", async () => {
      findWhen((req) => req.term === "日本");
      const wrapper = await mountView();
      await readingButton(wrapper).trigger("click");
      await wrapper.get("input.search-input").setValue("日本");
      await vi.waitFor(() =>
        expect(wrapper.findAll(".accordion-item")).toHaveLength(1),
      );
      expect(searches.slice(0, 2)).toMatchObject([
        { term: "", reading: "日本" },
        { term: "日本", reading: "" },
      ]);
      expect(hint(wrapper)).toContain(
        "Nothing by reading: searching 日本 as a word",
      );
      wrapper.unmount();
    });

    it("searches a character's Hangul reading as Hangul, as the Characters view links it", async () => {
      lang.value = "ko-KR";
      findWhen(() => false);
      const wrapper = await mountView();
      // the Characters view's link for 光 read 광 (Hanja's, or its own Hangul)
      await wrapper.get("input.search-input").setValue("*<光>{광}");
      expect(await firstSearch()).toMatchObject({
        ch: "光",
        ch_reading: "광",
        romanized: false,
      });
      expect(hint(wrapper)).toContain("Searching words with 光 read 광");
      // with a reading of its own, nothing to search the other way
      await new Promise((r) => setTimeout(r, 50));
      await flushPromises();
      expect(searches).toHaveLength(1);

      // a word with a Hangul reading of its own: not romanized either
      searches.length = 0;
      await wrapper.get("input.search-input").setValue("<光>{광}");
      expect(await firstSearch()).toMatchObject({
        term: "光",
        reading: "광",
        romanized: false,
      });
      await new Promise((r) => setTimeout(r, 50));
      await flushPromises();
      expect(searches).toHaveLength(1);
      wrapper.unmount();
    });

    it("searches Korean romanization in {} as romanization", async () => {
      lang.value = "ko-KR";
      findWhen(() => false);
      const wrapper = await mountView();
      await readingButton(wrapper).trigger("click");
      await wrapper.get("input.search-input").setValue("gwangsok");
      expect(await firstSearch()).toMatchObject({
        term: "gwangsok",
        reading: "",
        romanized: true,
      });
      expect(hint(wrapper)).toContain(
        "by reading gwangsok as Korean romanization",
      );
      // not searched another way: it is a reading already
      await new Promise((r) => setTimeout(r, 50));
      await flushPromises();
      expect(searches).toHaveLength(1);
      wrapper.unmount();
    });
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
    // and the dictionary's name, in case it has Han characters
    expect(
      wrapper.get('.tag[data-category="dictionary"]').attributes("lang"),
    ).toBe("zh");
    wrapper.unmount();
  });
  it("shows a word's pitch accents", async () => {
    mockIPC((cmd, args) => {
      if (cmd === "dict_styles") return null;
      if (cmd === "dicts") return [];
      if (cmd !== "search") throw new Error(`unexpected command: ${cmd}`);
      searches.push((args as { req: Record<string, unknown> }).req);
      const entry = {
        dict_id: 1,
        dict_path: "MyDict",
        lang: "ja-JP",
        target_lang: "en",
        term_id: 1,
        term: "橋",
        reading: "はし",
        def_tags: "",
        rules: "",
        score: 0,
        glossary: JSON.stringify(["bridge"]),
        sequence: null,
        tags: "",
      };
      const pitches = [
        {
          dict_id: 2,
          dict_path: "Kanjium",
          term: "橋",
          reading: "はし",
          pitches: [{ position: 2 }, { position: 0, tags: ["名"] }],
          tag_meta: {
            名: { category: "partOfSpeech", order: 0, notes: "noun" },
          },
        },
      ];
      return {
        items: [
          { id: 1, term: "橋", reading: " はし ", entries: [entry], pitches },
        ],
        next: null,
      };
    });
    const wrapper = await mountView();
    await wrapper.get("input.search-input").setValue("橋");
    await lastSearch();
    await flushPromises();
    const shown = wrapper.findAll(".pronunciation").map((p) => ({
      high: p
        .findAll('.pronunciation-mora[data-pitch="high"]')
        .map((m) => m.text()),
      drop: p
        .findAll('.pronunciation-mora[data-pitch-next="low"]')
        .map((m) => m.text()),
      text: p.text(),
    }));
    expect(shown).toEqual([
      // odaka: drops after し
      { high: ["し"], drop: ["し"], text: "はし[2]" },
      { high: ["し"], drop: [], text: "名はし[0]" },
    ]);
    // the dictionary first, as in Yomitan
    expect(
      wrapper
        .get(
          '.pronunciation-group .tag[data-category="pronunciation-dictionary"]',
        )
        .text(),
    ).toBe("Kanjium");
    // the tag, with what the tag bank says
    const tag = wrapper.get(
      '.pronunciation .tag[data-category="partOfSpeech"]',
    );
    expect(describedText(tag.element)).toBe("noun");
    // the headword's patterns, for custom CSS, as Yomitan puts them
    expect(
      wrapper.get(".headword").attributes("data-pronunciation-categories"),
    ).toBe("odaka heiban");
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
    // each in its dictionary's language, so Han characters take its forms
    expect(
      wrapper
        .findAll(".dict-filter-list span")
        .map((s) => s.attributes("lang") ?? null),
    ).toEqual([null, "zh", "zh-TW"]);
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
