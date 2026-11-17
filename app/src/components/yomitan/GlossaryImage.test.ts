import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import GlossaryImage, { type ImageData } from "./GlossaryImage.vue";

function mountImage(image: Omit<ImageData, "path">) {
  return mount(GlossaryImage, {
    props: {
      image: { path: "img/a.png", ...image },
      dictionary: "dict",
      getAssetSrc: (dict: string, path: string) => `dict://${dict}/${path}`,
    },
  });
}

/** the container's width and aspect ratio */
function size(wrapper: ReturnType<typeof mountImage>) {
  const style = (wrapper.get(".gloss-image-container").element as HTMLElement)
    .style;
  return [style.width, style.aspectRatio];
}

/** fire the image's load event, as a file of this pixel size */
async function load(
  wrapper: ReturnType<typeof mountImage>,
  width: number,
  height: number,
) {
  const img = wrapper.get("img");
  Object.defineProperty(img.element, "naturalWidth", { value: width });
  Object.defineProperty(img.element, "naturalHeight", { value: height });
  await img.trigger("load");
}

describe("GlossaryImage", () => {
  it("sizes in em when sizeUnits is em", () => {
    expect(size(mountImage({ width: 2, height: 1, sizeUnits: "em" }))).toEqual([
      "2em",
      "1 / 0.5",
    ]);
  });

  it("sizes in px by default", () => {
    expect(size(mountImage({ width: 100, height: 50 }))).toEqual([
      "100px",
      "1 / 0.5",
    ]);
  });

  it("takes a missing size from the loaded image", async () => {
    const wrapper = mountImage({ sizeUnits: "em" });
    // a guess until the image loads; em only applies to sizes the dictionary gives
    expect(size(wrapper)).toEqual(["100px", "1 / 1"]);
    await load(wrapper, 40, 20);
    expect(size(wrapper)).toEqual(["40px", "1 / 0.5"]);
  });

  it("keeps the image's aspect ratio when only the height is given", async () => {
    const wrapper = mountImage({ height: 30 });
    await load(wrapper, 60, 30);
    expect(size(wrapper)).toEqual(["60px", "1 / 0.5"]);
  });

  it("swaps width and height given the other way round", async () => {
    // a 1:2 image in a vertical-text span, sized for vertical writing
    const wrapper = mountImage({ width: 1, height: 0.5, sizeUnits: "em" });
    expect(size(wrapper)).toEqual(["1em", "1 / 0.5"]);
    await load(wrapper, 100, 200);
    expect(size(wrapper)).toEqual(["0.5em", "1 / 2"]);
  });

  it("doesn't swap a size that matches the image", async () => {
    const wrapper = mountImage({ width: 2, height: 1, sizeUnits: "em" });
    await load(wrapper, 200, 100);
    expect(size(wrapper)).toEqual(["2em", "1 / 0.5"]);
  });
});
