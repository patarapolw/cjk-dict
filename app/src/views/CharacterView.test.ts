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

let calls: { cmd: string; args: Record<string, unknown> }[] = [];

beforeEach(() => {
  calls = [];
  mockIPC((cmd, args) => {
    const a = args as Record<string, unknown>;
    calls.push({ cmd, args: a });
    if (cmd === "han_chars") {
      return [...String(a.text)].map((c) => HAN[c]).filter(Boolean);
    }
    if (cmd === "han_containing") {
      const page = (items: string[], next: number | null) => ({
        items: items.map((ch) => ({ ch, uses: 1 })),
        next,
      });
      if (a.components === "林木") return page(["森"], null);
      if (a.components !== "木") return page([], null);
      if (a.direct) return page(["林"], null);
      // at any depth, a page of two: 森 is made of 林
      return a.offset ? page(["杏"], null) : page(["林", "森"], 2);
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
    expect(wrapper.get(".search-hint").text()).toContain("Add characters");
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
    ).toEqual(["林"]);
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
    expect(wrapper.find('[data-char="林"] .more-used-in').exists()).toBe(false);
    expect(
      wrapper
        .findAll('[data-char="木"] .used-in-list .component')
        .map((b) => b.text()),
    ).toEqual(["林"]);
    // and below, those made of both
    expect(wrapper.get(".search-hint").text()).toContain("made of 林 + 木");
    expect(wrapper.get(".containing h2").text()).toContain("林 + 木");
    expect(wrapper.get(".containing").text()).toContain("all 2 characters");
    expect(wrapper.findAll(".char-grid button").map((b) => b.text())).toEqual([
      "森",
    ]);
    wrapper.unmount();
  });

  it("searches the characters made of one at any depth with More", async () => {
    const { wrapper, router } = await mountView("/kanji?q=木");
    await loaded(wrapper);

    const card = () => wrapper.get('[data-char="木"]');
    const usedIn = () =>
      card()
        .findAll(".used-in-list .component")
        .map((b) => b.text());
    expect(card().get(".used-in h3").text()).toBe("Used in");
    expect(usedIn()).toEqual(["林"]);

    await card().get(".more-used-in").trigger("click");
    await flushPromises();
    expect(card().get(".used-in h3").text()).toBe("Used in, at any depth");
    expect(usedIn()).toEqual(["林", "森"]);

    // then page by page
    await card().get(".more-used-in").trigger("click");
    await flushPromises();
    expect(usedIn()).toEqual(["林", "森", "杏"]);
    expect(card().find(".more-used-in").exists()).toBe(false);

    await card().findAll(".used-in-list .component")[1].trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.query.q).toBe("森");
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
