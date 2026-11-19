import { mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import type { IHanChar } from "../db/search";
import { lang } from "../util/settings";
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

/** the characters made directly of 木: more than a page of 20 */
const MADE_OF_MU = [
  "林",
  "森",
  "杏",
  ...Array.from({ length: 22 }, (_, i) => String.fromCodePoint(0x4e00 + i)),
];

let calls: { cmd: string; args: Record<string, unknown> }[] = [];

beforeEach(() => {
  calls = [];
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
        };
      };
      if (a.components === "林木" && !a.direct) return page(["森"]);
      if (a.components === "木" && a.direct) return page(MADE_OF_MU);
      return page([]);
    }
    if (cmd === "search") return { items: [], next: null };
    throw new Error(`unexpected command: ${cmd}`);
  });
});

afterEach(() => {
  lang.value = "";
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
    ).toEqual(MADE_OF_MU);
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
    expect(button.text()).toBe("* 木");
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

  it("looks up only the Han characters of what is typed", async () => {
    const { wrapper } = await mountView("/kanji");
    await wrapper.get("input.search-input").setValue("林 and かな");
    await loaded(wrapper);
    expect(calls.find((c) => c.cmd === "han_chars")?.args).toEqual({
      text: "林",
    });
    wrapper.unmount();
  });
});
