import { mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it } from "vitest";

import { DEFAULT_CREDITS } from "../util/credits";
import { dictList, type IDictInfo } from "../util/dicts";
import AboutView from "./AboutView.vue";

const credits = (
  attribution: string | null,
  url: string | null = null,
): NonNullable<IDictInfo["credits"]> => ({
  author: null,
  url,
  description: null,
  attribution,
});

const dict = (
  name: string,
  c: ReturnType<typeof credits>,
  extra: Record<string, unknown> = {},
) => ({
  name,
  title: name,
  revision: "1",
  lang: "ja-JP",
  target_lang: null,
  term_count: 10,
  credits: c,
  ...extra,
});

let dicts: ReturnType<typeof dict>[] = [];

beforeEach(() => {
  dictList.value = [];
  mockIPC((cmd) => {
    if (cmd === "dicts") return dicts;
    if (cmd === "plugin:app|version") return "0.1.0";
    throw new Error(`unexpected command: ${cmd}`);
  });
});

async function mountView() {
  const wrapper = mount(AboutView);
  await flushPromises();
  return wrapper;
}

const article = (w: Awaited<ReturnType<typeof mountView>>, name: string) =>
  w.get(`.credit[data-dictionary="${name}"]`);

describe("AboutView", () => {
  it("credits Kanjium as it asks, once", async () => {
    dicts = [
      dict(
        "Kanjium",
        // as dict-admin writes it: the credit, then the license and link
        credits(
          `${DEFAULT_CREDITS.Kanjium.credit!.replace(/\.$/, "")} (CC BY-SA 4.0): https://github.com/mifunetoshiro/kanjium`,
        ),
        { term_count: 0, pitch_count: 5 },
      ),
    ];
    const w = await mountView();
    expect(w.text()).toContain("Version 0.1.0");
    const a = article(w, "Kanjium");
    const quotes = a.findAll(".attribution");
    expect(quotes).toHaveLength(1);
    expect(quotes[0].text()).toBe(
      "The pitch accent notation, verb particle data, phonetics, homonyms and other additions or modifications to EDICT, KANJIDIC or KRADFILE were provided by Uros O. through his free database.",
    );
    expect(a.text()).toContain("CC BY-SA 4.0");
    w.unmount();
  });

  it("gives pixiv's terms, which aren't an open license", async () => {
    dicts = [dict("Pixiv", credits(null))];
    const w = await mountView();
    const a = article(w, "Pixiv");
    expect(a.text()).toContain("not an open license");
    expect(a.find('a[href="https://dic.pixiv.net/"]').exists()).toBe(true);
    expect(a.find('a[href="https://policies.pixiv.net/"]').exists()).toBe(true);
    w.unmount();
  });

  it("credits another dictionary as its index.json says", async () => {
    dicts = [
      dict("MyDict", {
        author: "Someone",
        url: "https://example.com/mydict",
        description: null,
        attribution: "Credit me.",
      }),
    ];
    const w = await mountView();
    const a = article(w, "MyDict");
    expect(a.get(".attribution").text()).toBe("Credit me.");
    expect(a.text()).toContain("Someone");
    expect(a.find('a[href="https://example.com/mydict"]').exists()).toBe(true);
    expect(a.text()).not.toContain("License");
    w.unmount();
  });

  it("lists the other data, with their licenses", async () => {
    dicts = [];
    const w = await mountView();
    expect(w.text()).toContain("No dictionaries downloaded yet.");
    expect(w.text()).toContain("HanaMinAFDKO");
    expect(w.text()).toContain("CC BY-SA 2.0 KR");
    w.unmount();
  });
});
