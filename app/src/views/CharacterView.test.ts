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
    variants: [],
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
          ["korean_h", "림"],
          ["nanori", "しげ"],
          ["pinyin", "lín"],
          ["strokes", "8"],
        ],
        tag_meta: {
          pinyin: { category: "reading", order: 0, notes: "Pinyin" },
          korean_h: { category: "reading", order: 1, notes: "Korean" },
          nanori: { category: "reading", order: 2, notes: "Name readings" },
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
    variants: [],
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
  variants: [],
  entries: [daoEntry("KANJIDIC", null), daoEntry("CC-CEDICT.Hanzi", "zh")],
};

/** 学 has no entry of its own, its traditional form 學 has */
HAN["学"] = {
  ch: "学",
  ids: [],
  components: [],
  all_components: [],
  hangul: [],
  uses: 50,
  variants: [
    { ch: "學", kinds: ["traditional", "kyujitai"] },
    { ch: "斈", kinds: ["semantic"] },
  ],
  entries: [],
};
HAN["學"] = {
  ch: "學",
  ids: [],
  components: [],
  all_components: [],
  hangul: ["학"],
  uses: 40,
  variants: [{ ch: "学", kinds: ["simplified", "shinjitai", "semantic"] }],
  entries: [
    {
      ...daoEntry("KANJIDIC", null),
      character: "學",
      onyomi: "ガク",
      meanings: ["learning"],
    },
  ],
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
            variants: [],
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
      const readings = a.readings as string[];
      const all = readings.includes("りん")
        ? ["林", "森"]
        : readings.includes("lim") && a.korean
          ? ["林"]
          : [];
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

/** opens a character's card, collapsed in a list */
async function openCard(
  wrapper: Awaited<ReturnType<typeof mountView>>["wrapper"],
  ch: string,
) {
  const toggle = wrapper.get(`[data-char="${ch}"] .char-toggle`);
  expect(toggle.attributes("aria-expanded")).toBe("false");
  await toggle.trigger("click");
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
    expect(card.get(".ids").text().replace(/\s/g, "")).toBe("⿰木木");
    // in the header, every reading but kun and name readings, one row per kind: on by
    // its script, KANJIDIC having no one language; its Hangul with the card's own
    const rows = (sel: string) =>
      card
        .findAll(`${sel} dt`)
        .map((dt) => [dt.text(), dt.element.nextElementSibling!.textContent!]);
    expect(rows(".card-readings")).toEqual([
      ["On", "リン"],
      ["Pinyin", "lín"],
      ["Hangul", "림"],
    ]);
    // inside, the rest
    expect(rows(".kanji-entry .readings")).toEqual([
      ["Kun", "はやし"],
      ["Name readings", "しげ"],
    ]);
    expect(card.findAll(".meanings li").map((li) => li.text())).toEqual([
      "grove",
      "forest",
    ]);
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

  it("lists a character's variants, each looked up when clicked", async () => {
    const { wrapper, router } = await mountView("/kanji?q=學");
    await loaded(wrapper);

    const card = wrapper.get('[data-char="學"]');
    // the loose kind only when there's nothing more definite
    expect(
      card
        .findAll(".variant-list li")
        .map((li) => [
          li.get(".component").text(),
          li.get(".variant-kind").text(),
        ]),
    ).toEqual([["学", "simplified, shinjitai"]]);
    // it has its own entry: its variants' aren't looked up
    expect(card.find(".via-note").exists()).toBe(false);
    expect(
      calls.filter((c) => c.cmd === "han_chars").map((c) => c.args.text),
    ).toEqual(["學"]);

    await card.get(".variant-list .component").trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.query.q).toBe("学");
    wrapper.unmount();
  });

  it("shows the variants' entries of a character with none, marked", async () => {
    const { wrapper, router } = await mountView("/kanji?q=学");
    await loaded(wrapper);

    await vi.waitFor(() =>
      expect(
        calls.filter((c) => c.cmd === "han_chars").map((c) => c.args.text),
      ).toEqual(["学", "學斈"]),
    );
    await flushPromises();
    const card = wrapper.get('[data-char="学"]');
    expect(card.get(".via-note").text()).toBe(
      "No entry for 学; its variants':",
    );
    const entry = card.get(".kanji-entry");
    expect(entry.attributes("data-via")).toBe("學");
    expect(entry.get(".via-variant").text()).toBe("學 traditional, kyūjitai");
    expect(entry.get(".meanings").text()).toBe("learning");

    // its readings, in the header, are 學's
    await card.get(".card-readings button.reading").trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.query.q).toBe("*<學>{ガク}");
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
    await openCard(wrapper, "林");
    await openCard(wrapper, "木");

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
    expect(button.text().replace(/\s+/g, " ")).toBe(
      "All characters made of 木",
    );
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
    // each collapsed, and in full when opened, with its own "Used in"
    expect(wrapper.find('[data-char="林"] .used-in').exists()).toBe(false);
    await openCard(wrapper, "林");
    expect(wrapper.find('[data-char="林"] .used-in').exists()).toBe(true);
    expect(wrapper.find(".containing").exists()).toBe(false);

    // the next page
    await wrapper.get(".super-more").trigger("click");
    await flushPromises();
    expect(cards()).toEqual(MADE_OF_MU);
    expect(wrapper.find(".super-more").exists()).toBe(false);
    wrapper.unmount();
  });

  it("collapses the cards of a list, with the gist outside, and keeps them as left", async () => {
    // not 林木: the cards open are kept per query, and another test opens those
    const { wrapper, router } = await mountView("/kanji?q=木林");
    await loaded(wrapper);

    const card = wrapper.get('[data-char="林"]');
    const toggle = card.get(".char-toggle");
    expect(toggle.attributes("aria-expanded")).toBe("false");
    expect(card.find(".char-details").exists()).toBe(false);
    // outside: the first meanings, how many words use it, the readings but kun
    expect(card.get(".gist-meanings").text()).toBe("grove; forest");
    expect(card.get(".uses").text()).toBe("in 120 words");
    expect(card.get(".card-readings").text()).toContain("リン");
    expect(card.get(".card-readings").text()).not.toContain("はやし");

    await toggle.trigger("click");
    expect(toggle.attributes("aria-expanded")).toBe("true");
    expect(card.get(".kanji-entry .meanings").text()).toContain("grove");

    // to Words and back: the view is mounted again, 林 still open
    await card
      .findAll(".item-actions button")
      .find((b) => b.text() === "= 林")!
      .trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.path).toBe("/dictionary");
    wrapper.unmount();

    calls = [];
    calls = [];
    await router.push("/kanji?q=木林");
    const again = mount(CharacterView, {
      global: { plugins: [router] },
      attachTo: document.body,
    });
    await loaded(again);
    const expanded = (ch: string) =>
      again.get(`[data-char="${ch}"] .char-toggle`).attributes("aria-expanded");
    expect(expanded("林")).toBe("true");
    expect(expanded("木")).toBe("false");
    again.unmount();
  });

  it("links a character to the words with it as a word and anywhere", async () => {
    const { wrapper, router } = await mountView("/kanji?q=林");
    await loaded(wrapper);

    // one character: open
    expect(
      wrapper.get('[data-char="林"] .char-toggle').attributes("aria-expanded"),
    ).toBe("true");
    const actions = wrapper.findAll('[data-char="林"] .item-actions button');
    expect(actions.map((b) => b.text())).toEqual(["= 林", "*林*"]);
    await actions[1].trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.path).toBe("/dictionary");
    expect(router.currentRoute.value.query).toMatchObject({
      q: "*林*",
      qState: "",
    });
    // no single-character word entries, the same as "= 林"
    expect(calls.some((c) => c.cmd === "search")).toBe(false);
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
      readings: ["りん"],
      pinyin: true,
      korean: true,
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

  it("looks romaji up in kana with Japanese selected", async () => {
    lang.value = "ja-JP";
    const { wrapper } = await mountView("/kanji");
    await wrapper.get("input.search-input").setValue("rin");
    await loaded(wrapper);
    expect(calls.find((c) => c.cmd === "han_by_reading")?.args).toMatchObject({
      readings: ["りん"],
      pinyin: true,
      korean: false,
    });
    expect(wrapper.get(".search-hint").text().replace(/\s+/g, " ")).toBe(
      "Characters read rin as りん",
    );
    expect(
      wrapper.findAll(".char-card").map((c) => c.attributes("data-char")),
    ).toEqual(["林", "森"]);
    wrapper.unmount();
  });

  it("takes Latin letters as Korean romanization with Korean selected", async () => {
    lang.value = "ko-KR";
    const { wrapper } = await mountView("/kanji");
    await wrapper.get("input.search-input").setValue("lim");
    await loaded(wrapper);
    expect(calls.find((c) => c.cmd === "han_by_reading")?.args).toMatchObject({
      readings: ["lim"],
      pinyin: false,
      korean: true,
    });
    expect(wrapper.get(".search-hint").text().replace(/\s+/g, " ")).toBe(
      "Characters read lim as Korean romanization",
    );
    expect(
      wrapper.findAll(".char-card").map((c) => c.attributes("data-char")),
    ).toEqual(["林"]);

    // with Chinese, pinyin
    lang.value = "zh";
    calls = [];
    await vi.waitFor(() =>
      expect(calls.some((c) => c.cmd === "han_by_reading")).toBe(true),
    );
    expect(calls.find((c) => c.cmd === "han_by_reading")?.args).toMatchObject({
      readings: ["lim"],
      pinyin: true,
      korean: false,
    });
    wrapper.unmount();
  });

  it("reads Latin letters every way with Auto-detect", async () => {
    const { wrapper } = await mountView("/kanji");
    await wrapper.get("input.search-input").setValue("rin");
    await loaded(wrapper);
    // romaji in kana too, and pinyin and romanization alike
    expect(calls.find((c) => c.cmd === "han_by_reading")?.args).toMatchObject({
      readings: ["rin", "りん"],
      pinyin: true,
      korean: true,
    });
    expect(wrapper.get(".search-hint").text().replace(/\s+/g, " ")).toBe(
      "Characters read rin as りん, pinyin or Korean romanization",
    );
    expect(
      wrapper.findAll(".char-card").map((c) => c.attributes("data-char")),
    ).toEqual(["林", "森"]);

    // not romaji: pinyin or romanization
    calls = [];
    await wrapper.get("input.search-input").setValue("heung");
    await vi.waitFor(() =>
      expect(calls.some((c) => c.cmd === "han_by_reading")).toBe(true),
    );
    await flushPromises();
    expect(
      calls.find((c) => c.cmd === "han_by_reading")?.args.readings,
    ).toEqual(["heung"]);
    expect(wrapper.get(".search-hint").text().replace(/\s+/g, " ")).toBe(
      "Characters read heung as pinyin or Korean romanization",
    );
    wrapper.unmount();
  });

  it("takes a reading in braces, and says when no character has it", async () => {
    const { wrapper } = await mountView("/kanji");
    await wrapper.get("input.search-input").setValue("{xing}");
    await vi.waitFor(() =>
      expect(calls.some((c) => c.cmd === "han_by_reading")).toBe(true),
    );
    await flushPromises();
    expect(
      calls.find((c) => c.cmd === "han_by_reading")?.args.readings,
    ).toEqual(["xing"]);
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
