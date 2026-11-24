import { mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import type { IHanChar, IKanjiEntry } from "../db/search";
import { type IDictInfo, excludedCharDicts } from "../util/dicts";
import { lang } from "../util/settings";
import { touchScreen } from "../util/tooltip";
import CharacterView from "./CharacterView.vue";

const HAN: Record<string, IHanChar> = {
  林: {
    ch: "林",
    ids: [{ ids: "⿰木木", regions: "" }],
    components: ["木"],
    all_components: ["木"],
    hangul: ["림"],
    uses: 120,
    entries: [
      {
        dict_id: 1,
        dict_path: "KANJIDIC",
        lang: null,
        character: "林",
        onyomi: "リン",
        kunyomi: "はやし",
        tags: "jouyou",
        meanings: ["grove", "forest"],
        stats: [
          ["freq", "1000"],
          ["pinyin", "lín"],
          ["strokes", "8"],
        ],
        tag_meta: {
          pinyin: { category: "reading", order: 0, notes: "Pinyin" },
          jouyou: { category: "frequent", order: 0, notes: "regular-use" },
          strokes: { category: "misc", order: -1, notes: "Stroke count" },
          freq: { category: "misc", order: 0, notes: "Frequency" },
        },
      },
    ],
  },
  木: {
    ch: "木",
    ids: [{ ids: "木", regions: "" }],
    components: [],
    all_components: [],
    hangul: ["목"],
    uses: 300,
    entries: [],
  },
};

/** an entry of 道 in a kanji dictionary of `lang` */
const daoEntry = (dict_path: string, lang: string | null): IKanjiEntry => ({
  dict_id: 1,
  dict_path,
  lang,
  character: "道",
  onyomi: "",
  kunyomi: "",
  tags: "",
  meanings: [`${dict_path} meaning`],
  stats: [],
  tag_meta: {},
});

HAN["道"] = {
  ch: "道",
  ids: [],
  components: [],
  all_components: [],
  hangul: ["도"],
  uses: 10,
  entries: [daoEntry("KANJIDIC", null), daoEntry("CC-CEDICT.Hanzi", "zh")],
};

/** what the `dicts` command returns */
let dicts: Partial<IDictInfo>[] = [];

const kanjiDict = (name: string, lang: string | null) => ({
  name,
  title: name,
  lang,
  term_count: 0,
  char_count: 10,
});

/** the characters made directly of 木: more than "Used in" shows (30), and than a page
 * of `*木` (20) */
const MADE_OF_MU = [
  "林",
  "森",
  "杏",
  ...Array.from({ length: 32 }, (_, i) => String.fromCodePoint(0x4e00 + i)),
];

let calls: { cmd: string; args: Record<string, unknown> }[] = [];

beforeEach(() => {
  calls = [];
  dicts = [];
  mockIPC((cmd, args) => {
    const a = args as Record<string, unknown>;
    calls.push({ cmd, args: a });
    if (cmd === "han_chars") {
      return [...String(a.text)].map(
        (ch): IHanChar =>
          HAN[ch] ?? {
            ch,
            ids: [],
            components: [],
            all_components: [],
            hangul: [],
            uses: 0,
            entries: [],
          },
      );
    }
    if (cmd === "han_containing") {
      const page = (all: string[]) => {
        const offset = Number(a.offset);
        const end = offset + Number(a.limit);
        return {
          items: all.slice(offset, end).map((ch) => ({ ch, uses: 1 })),
          next: end < all.length ? end : null,
          total: all.length,
        };
      };
      if (a.components === "林木" && !a.direct) return page(["森"]);
      if (a.components === "木" && a.direct) return page(MADE_OF_MU);
      return page([]);
    }
    if (cmd === "han_by_reading") {
      const all = a.reading === "りん" ? ["林", "森"] : [];
      return {
        items: all.map((ch) => ({ ch, uses: 1 })),
        next: null,
        total: all.length,
      };
    }
    if (cmd === "search") return { items: [], next: null };
    if (cmd === "dicts") return dicts;
    throw new Error(`unexpected command: ${cmd}`);
  });
});

afterEach(() => {
  lang.value = "";
  excludedCharDicts.value = [];
  touchScreen.value = false;
});

async function mountView(path: string) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/kanji", component: CharacterView },
      { path: "/dictionary", component: { template: "<div />" } },
    ],
  });
  await router.push(path);
  await router.isReady();
  const wrapper = mount(CharacterView, {
    global: { plugins: [router] },
    attachTo: document.body,
  });
  return { wrapper, router };
}

async function loaded(
  wrapper: Awaited<ReturnType<typeof mountView>>["wrapper"],
) {
  await vi.waitFor(() =>
    expect(calls.some((c) => c.cmd === "han_chars")).toBe(true),
  );
  await flushPromises();
  return wrapper;
}

describe("CharacterView", () => {
  it("searches the words with a reading when it is clicked", async () => {
    lang.value = "ja-JP";
    const { wrapper, router } = await mountView("/kanji?q=林");
    await loaded(wrapper);

    const reading = (text: string) =>
      wrapper.findAll("button.reading").find((b) => b.text() === text)!;
    await reading("はやし").trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.path).toBe("/dictionary");
    expect(router.currentRoute.value.query).toMatchObject({
      q: "*<林>{はやし}",
      qState: "",
    });

    // pinyin is Chinese: the search language follows
    await router.push("/kanji?q=林");
    await loaded(wrapper);
    await reading("lín").trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.query.q).toBe("*<林>{lín}");
    expect(lang.value).toBe("zh");
    wrapper.unmount();
  });

  it("shows a character's breakdown and kanji entry", async () => {
    const { wrapper } = await mountView("/kanji?q=林");
    await loaded(wrapper);

    const card = wrapper.get('[data-char="林"]');
    expect(card.get(".glyph").text()).toBe("林");
    expect(card.get(".hangul").text()).toBe("림");
    expect(card.get(".ids").text().replace(/\s/g, "")).toBe("⿰木木");
    expect(card.get(".readings").text()).toContain("リン");
    // on and kun by their script, KANJIDIC having no one language
    expect(card.findAll(".readings dt").map((dt) => dt.text())).toEqual([
      "On",
      "Kun",
      "Pinyin",
    ]);
    expect(card.findAll(".meanings li").map((li) => li.text())).toEqual([
      "grove",
      "forest",
    ]);
    // other languages' readings go with the readings
    expect(card.get(".readings").text()).toContain("Pinyin");
    expect(card.get(".readings").text()).toContain("lín");
    // the rest by the tag bank's order
    expect(card.findAll(".stats th").map((th) => th.text())).toEqual([
      "Stroke count",
      "Frequency",
    ]);
    // one character: no search for characters made of all of them
    expect(
      calls.filter((c) => c.cmd === "han_containing").map((c) => c.args),
    ).toEqual([{ components: "林", limit: 30, offset: 0, direct: true }]);
    expect(wrapper.find(".containing").exists()).toBe(false);
    // nothing but what is shown
    expect(wrapper.get(".search-hint").text()).toBe("Character 林");
    wrapper.unmount();
  });

  it("looks up a component when it is clicked, and lists characters made of it", async () => {
    const { wrapper, router } = await mountView("/kanji?q=林");
    await loaded(wrapper);

    calls = [];
    await wrapper.get(".ids .component").trigger("click");
    await vi.waitFor(() =>
      expect(calls.some((c) => c.cmd === "han_chars")).toBe(true),
    );
    await flushPromises();
    expect(router.currentRoute.value.query.q).toBe("木");
    expect(wrapper.get('[data-char="木"]').text()).toContain(
      "Does not break down further",
    );
    expect(
      wrapper
        .findAll('[data-char="木"] .used-in-list .component')
        .map((b) => b.text()),
    ).toEqual(MADE_OF_MU.slice(0, 30));
    expect(wrapper.get('[data-char="木"] .used-in-more').text()).toBe("5 more");
    wrapper.unmount();
  });

  it("lists the characters made directly of each character on its card", async () => {
    const { wrapper } = await mountView("/kanji?q=林木");
    await loaded(wrapper);

    const direct = calls.filter(
      (c) => c.cmd === "han_containing" && c.args.direct,
    );
    expect(direct.map((c) => c.args.components)).toEqual(["林", "木"]);
    expect(wrapper.get('[data-char="林"] .used-in').text()).toContain(
      "Not a component of another character",
    );
    // nothing to search further
    expect(wrapper.find('[data-char="林"] .super-search').exists()).toBe(false);
    expect(
      wrapper
        .findAll('[data-char="木"] .used-in-list .component')
        .map((b) => b.text())
        .slice(0, 2),
    ).toEqual(["林", "森"]);
    // and below, those made of both
    expect(wrapper.get(".search-hint").text()).toBe("Characters 林 木");
    expect(wrapper.get(".containing h2").text()).toContain("林 + 木");
    expect(wrapper.get(".containing").text()).toContain("all 2 characters");
    expect(wrapper.findAll(".char-grid button").map((b) => b.text())).toEqual([
      "森",
    ]);
    wrapper.unmount();
  });

  it("shows the characters made directly of one in full with *", async () => {
    const { wrapper, router } = await mountView("/kanji?q=木");
    await loaded(wrapper);

    calls = [];
    const button = wrapper.get('[data-char="木"] .super-search');
    // as it will read in the search box
    expect(button.text().replace(/\s+/g, " ")).toBe("Search *木");
    await button.trigger("click");
    await vi.waitFor(() =>
      expect(calls.some((c) => c.cmd === "han_chars")).toBe(true),
    );
    await flushPromises();
    expect(router.currentRoute.value.query.q).toBe("*木");
    expect(wrapper.get(".search-hint").text()).toBe(
      "Characters made directly of 木",
    );
    expect(calls.find((c) => c.cmd === "han_containing")?.args).toEqual({
      components: "木",
      limit: 20,
      offset: 0,
      direct: true,
    });
    const cards = () =>
      wrapper.findAll(".char-card").map((c) => c.attributes("data-char"));
    expect(cards()).toEqual(MADE_OF_MU.slice(0, 20));
    // each in full, with its own "Used in"
    expect(wrapper.find('[data-char="林"] .used-in').exists()).toBe(true);
    expect(wrapper.find(".containing").exists()).toBe(false);

    // the next page
    await wrapper.get(".super-more").trigger("click");
    await flushPromises();
    expect(cards()).toEqual(MADE_OF_MU);
    expect(wrapper.find(".super-more").exists()).toBe(false);

    // one of them alone
    await wrapper
      .findAll('[data-char="林"] .item-actions button')
      .find((b) => b.text() === "字 林")!
      .trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.query.q).toBe("林");
    wrapper.unmount();
  });

  it("says on a touchscreen, before anything is typed, that a long press explains", async () => {
    touchScreen.value = true;
    const { wrapper } = await mountView("/kanji");
    expect(wrapper.find(".search-hint .touch-help").exists()).toBe(true);
    await wrapper.get("input.search-input").setValue("林");
    await loaded(wrapper);
    expect(wrapper.find(".search-hint .touch-help").exists()).toBe(false);
    wrapper.unmount();
  });

  it("looks up only the Han characters of what is typed", async () => {
    const { wrapper } = await mountView("/kanji");
    await wrapper.get("input.search-input").setValue("林 and かな");
    await loaded(wrapper);
    expect(calls.find((c) => c.cmd === "han_chars")?.args).toEqual({
      text: "林",
    });
    wrapper.unmount();
  });

  it("shows the characters read so for a reading, each in full", async () => {
    const { wrapper } = await mountView("/kanji");
    await wrapper.get("input.search-input").setValue("りん");
    await loaded(wrapper);
    expect(calls.find((c) => c.cmd === "han_by_reading")?.args).toEqual({
      reading: "りん",
      limit: 20,
      offset: 0,
    });
    expect(calls.find((c) => c.cmd === "han_chars")?.args).toEqual({
      text: "林森",
    });
    expect(wrapper.get(".search-hint").text()).toBe("Characters read りん");
    expect(
      wrapper.findAll(".char-card").map((c) => c.attributes("data-char")),
    ).toEqual(["林", "森"]);
    expect(wrapper.find(".containing").exists()).toBe(false);
    wrapper.unmount();
  });

  it("takes a reading in braces, and says when no character has it", async () => {
    const { wrapper } = await mountView("/kanji");
    await wrapper.get("input.search-input").setValue("{xing}");
    await vi.waitFor(() =>
      expect(calls.some((c) => c.cmd === "han_by_reading")).toBe(true),
    );
    await flushPromises();
    expect(calls.find((c) => c.cmd === "han_by_reading")?.args.reading).toBe(
      "xing",
    );
    expect(wrapper.get(".no-results").text()).toBe("No characters read xing");

    // `*` and Han characters are not readings
    calls = [];
    await wrapper.get("input.search-input").setValue("*林");
    await vi.waitFor(() =>
      expect(calls.some((c) => c.cmd === "han_containing")).toBe(true),
    );
    await flushPromises();
    expect(calls.some((c) => c.cmd === "han_by_reading")).toBe(false);
    expect(wrapper.find(".no-results").exists()).toBe(false);
    wrapper.unmount();
  });

  describe("kanji dictionaries", () => {
    const meanings = (
      wrapper: Awaited<ReturnType<typeof mountView>>["wrapper"],
    ) =>
      wrapper.findAll('[data-char="道"] .meanings li').map((li) => li.text());

    it("shows those of the search language and of none, as Words does", async () => {
      dicts = [kanjiDict("KANJIDIC", null), kanjiDict("CC-CEDICT.Hanzi", "zh")];
      const { wrapper } = await mountView("/kanji?q=道");
      await loaded(wrapper);
      expect(meanings(wrapper)).toEqual([
        "KANJIDIC meaning",
        "CC-CEDICT.Hanzi meaning",
      ]);
      expect(
        wrapper.findAll(".dict-filter-list span").map((s) => s.text()),
      ).toEqual(["KANJIDIC", "CC-CEDICT.Hanzi"]);

      // Japanese: CC-CEDICT.Hanzi is Chinese; KANJIDIC, of no one language, stays
      lang.value = "ja-JP";
      await flushPromises();
      expect(meanings(wrapper)).toEqual(["KANJIDIC meaning"]);
      // one dictionary: nothing to choose
      expect(wrapper.find(".dict-filter").exists()).toBe(false);

      // Chinese: its own first
      lang.value = "zh";
      await flushPromises();
      expect(meanings(wrapper)).toEqual([
        "CC-CEDICT.Hanzi meaning",
        "KANJIDIC meaning",
      ]);
      wrapper.unmount();
    });

    it("leaves out the dictionaries unticked, keeps one ticked, and remembers them", async () => {
      dicts = [kanjiDict("KANJIDIC", null), kanjiDict("CC-CEDICT.Hanzi", "zh")];
      const { wrapper } = await mountView("/kanji?q=道");
      await loaded(wrapper);
      const boxes = wrapper.findAll(".dict-filter-list input");
      await boxes[0].setValue(false);
      expect(meanings(wrapper)).toEqual(["CC-CEDICT.Hanzi meaning"]);
      expect(wrapper.get(".dict-filter-count").text()).toBe("(1 of 2)");
      // the last one ticked can't be unticked
      expect(boxes[1].attributes("disabled")).toBeDefined();
      expect(
        JSON.parse(localStorage.getItem("cjk-dict.excludedCharDicts")!),
      ).toEqual(["KANJIDIC"]);

      await wrapper.get(".dict-filter .link-button").trigger("click");
      expect(excludedCharDicts.value).toEqual([]);
      expect(meanings(wrapper)).toHaveLength(2);
      wrapper.unmount();
    });

    it("warns when the search language's are all left out", async () => {
      dicts = [kanjiDict("KANJIDIC", null), kanjiDict("CC-CEDICT.Hanzi", "zh")];
      excludedCharDicts.value = ["KANJIDIC"];
      lang.value = "ja-JP";
      const { wrapper } = await mountView("/kanji?q=道");
      await loaded(wrapper);
      expect(meanings(wrapper)).toEqual([]);
      expect(wrapper.get(".dict-filter-warning").text()).toContain(
        "see its entries",
      );
      // listed even alone, so that it can be ticked again
      await wrapper.get(".dict-filter-list input").setValue(true);
      expect(meanings(wrapper)).toEqual(["KANJIDIC meaning"]);
      expect(wrapper.find(".dict-filter-warning").exists()).toBe(false);
      wrapper.unmount();
    });
  });
});
