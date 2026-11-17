import { afterEach, describe, expect, it } from "vitest";

import { scrollContainer } from "./scroll";

describe("scrollContainer", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("is the element itself when it scrolls", () => {
    document.body.innerHTML = `<div id="list" style="overflow: auto"></div>`;
    const list = document.getElementById("list")!;
    expect(scrollContainer(list)).toBe(list);
  });

  it("is the nearest scrolling ancestor otherwise, as in a short window", () => {
    document.body.innerHTML = `
      <div id="page" style="overflow-y: auto">
        <main><div id="list" style="overflow: visible"></div></main>
      </div>`;
    expect(scrollContainer(document.getElementById("list")!)).toBe(
      document.getElementById("page"),
    );
  });

  it("is the document when nothing else scrolls", () => {
    document.body.innerHTML = `<div id="list"></div>`;
    expect(scrollContainer(document.getElementById("list")!)).toBe(
      document.scrollingElement ?? document.documentElement,
    );
  });
});
