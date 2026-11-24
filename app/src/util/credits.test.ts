import { describe, expect, it } from "vitest";

import src from "../../../dict-admin/src/defaults.rs?raw";

import { DEFAULT_CREDITS } from "./credits";

/** the default dictionaries' names, as dict-admin's defaults.rs lists them */
function defaultNames(): string[] {
  const names = [...src.matchAll(/^\s*name: "([^"]+)",/gm)].map((m) => m[1]);
  // fixed sources: ("Kanjium", Some("ja-JP"), None, ...)
  names.push(
    ...[
      ...src.matchAll(
        /^\s*\("([^"]+)", Some\("[^"]+"\), (?:None|Some\("[^"]+"\)), crate::/gm,
      ),
    ].map((m) => m[1]),
  );
  // wty_name()'s names
  names.push(...[...src.matchAll(/=> "(wty-[^"]+)"/g)].map((m) => m[1]));
  return names;
}

describe("DEFAULT_CREDITS", () => {
  it("credits every default dictionary, and only those", () => {
    const names = defaultNames();
    expect(names).toContain("Kanjium");
    expect(names).toContain("Hanja");
    expect(names).toContain("wty-zh-en");
    expect(Object.keys(DEFAULT_CREDITS).sort()).toEqual([...names].sort());
  });
});
