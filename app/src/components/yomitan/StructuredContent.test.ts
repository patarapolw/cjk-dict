import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import { describedText } from "../../util/tooltip";
import StructuredContent from "./StructuredContent";

function render(node: unknown) {
  return mount(StructuredContent, {
    props: {
      node: node as any,
      dictionary: "dict",
      getAssetSrc: (dict: string, path: string) => `dict://${dict}/${path}`,
    },
    attachTo: document.body,
  });
}

describe("StructuredContent", () => {
  it("renders only the schema's elements", () => {
    const wrapper = render({
      tag: "div",
      content: [
        "a",
        { tag: "script", content: "alert(1)" },
        { tag: "iframe", src: "https://example.com" },
        { tag: "span", content: "b" },
      ],
    });
    const div = wrapper.get("div.gloss-sc-div");
    expect(div.find("script").exists()).toBe(false);
    expect(div.find("iframe").exists()).toBe(false);
    expect(div.text()).toBe("ab");
    expect(div.get("span").classes()).toEqual(["gloss-sc-span"]);
    wrapper.unmount();
  });

  it("applies only the schema's style keys", () => {
    const wrapper = render({
      tag: "span",
      style: {
        color: "red",
        marginTop: 1,
        // numbers are only em for margins
        fontSize: 2,
        position: "fixed",
        textDecorationLine: ["underline", "overline"],
      },
      // not attributes of the schema
      onclick: "alert(1)",
      id: "x",
      content: "a",
    });
    const span = wrapper.get("span").element as HTMLElement;
    expect(span.style.color).toBe("red");
    expect(span.style.marginTop).toBe("1em");
    expect(span.style.fontSize).toBe("");
    expect(span.style.position).toBe("");
    expect(span.style.textDecoration).toBe("underline overline");
    expect(span.getAttribute("onclick")).toBeNull();
    expect(span.id).toBe("");
    wrapper.unmount();
  });

  it("turns data into data-sc-* attributes", () => {
    const wrapper = render({
      tag: "span",
      data: { content: "glossary", subClass: "x", bad: 1, "a b": "y" },
      content: "a",
    });
    const span = wrapper.get("span").element;
    expect(span.getAttribute("data-sc-content")).toBe("glossary");
    expect(span.getAttribute("data-sc-sub-class")).toBe("x");
    // only string values, and only names that are valid attributes
    expect(span.hasAttribute("data-sc-bad")).toBe(false);
    expect(
      [...span.attributes].map((a) => a.name).filter((n) => n.includes(" ")),
    ).toEqual([]);
    wrapper.unmount();
  });

  it("links only http(s) URLs", () => {
    const wrapper = render([
      { tag: "a", href: "javascript:alert(1)", content: "js" },
      { tag: "a", href: "https://example.com", content: "web" },
      { tag: "a", href: "?query=語", content: "internal" },
    ]);
    const links = wrapper.findAll("a");
    expect(links.map((a) => a.attributes("href"))).toEqual([
      undefined,
      "https://example.com",
      undefined,
    ]);
    wrapper.unmount();
  });

  it("shows a title as a tooltip a touchscreen shows too", () => {
    const wrapper = render({
      tag: "span",
      title: "Godan verb",
      content: "v5",
    });
    const span = wrapper.get("span.gloss-sc-span");
    expect(span.attributes("title")).toBeUndefined();
    // for screen readers
    expect(describedText(span.element)).toBe("Godan verb");
    expect(span.classes()).toContain("has-tooltip");
    wrapper.unmount();
  });
});
