import { mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h, nextTick, ref, withDirectives } from "vue";

import Tooltip from "../components/Tooltip.vue";
import {
  HOVER_MS,
  LONG_PRESS_MS,
  TOUCH_SHOW_MS,
  hideTooltip,
  tooltip,
  vTooltip,
} from "./tooltip";

function pointer(
  el: Element,
  type: string,
  pointerType: string,
  at = { x: 0, y: 0 },
) {
  const e = new MouseEvent(type, {
    bubbles: !type.endsWith("enter") && !type.endsWith("leave"),
    cancelable: true,
    clientX: at.x,
    clientY: at.y,
  });
  Object.defineProperty(e, "pointerType", { value: pointerType });
  el.dispatchEvent(e);
}

const text = ref<string | undefined>("Words containing it");
const clicks = vi.fn();

function mountButton() {
  const Comp = defineComponent(() => () => [
    withDirectives(h("button", { onClick: clicks }, "~ 林"), [
      [vTooltip, text.value],
    ]),
    h("p", "elsewhere"),
    h(Tooltip),
  ]);
  return mount(Comp, { attachTo: document.body });
}

beforeEach(() => {
  vi.useFakeTimers();
  text.value = "Words containing it";
  clicks.mockClear();
});

afterEach(() => {
  hideTooltip();
  vi.useRealTimers();
});

describe("v-tooltip", () => {
  it("shows on a mouse resting on the element, and hides when it leaves", async () => {
    const wrapper = mountButton();
    const button = wrapper.get("button").element;
    expect(button.getAttribute("title")).toBeNull();
    expect(button.getAttribute("aria-description")).toBe("Words containing it");

    pointer(button, "pointerenter", "mouse");
    vi.advanceTimersByTime(HOVER_MS - 1);
    expect(tooltip.value).toBeNull();
    vi.advanceTimersByTime(1);
    await nextTick();
    await nextTick();
    expect(wrapper.get('[role="tooltip"]').text()).toBe("Words containing it");

    pointer(button, "pointerleave", "mouse");
    await nextTick();
    expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("shows on a long press, which then isn't a tap", async () => {
    const wrapper = mountButton();
    const button = wrapper.get("button").element;

    pointer(button, "pointerdown", "touch");
    vi.advanceTimersByTime(LONG_PRESS_MS);
    await nextTick();
    expect(tooltip.value?.text).toBe("Words containing it");
    pointer(button, "pointerup", "touch");
    button.click();
    expect(clicks).not.toHaveBeenCalled();

    // until a touch elsewhere
    pointer(wrapper.get("p").element, "pointerdown", "touch");
    expect(tooltip.value).toBeNull();

    // a tap is still a tap
    pointer(button, "pointerdown", "touch");
    vi.advanceTimersByTime(100);
    pointer(button, "pointerup", "touch");
    button.click();
    expect(clicks).toHaveBeenCalledTimes(1);
    expect(tooltip.value).toBeNull();
    wrapper.unmount();
  });

  it("hides a long press's tooltip after a while, and doesn't show for a swipe", async () => {
    const wrapper = mountButton();
    const button = wrapper.get("button").element;

    pointer(button, "pointerdown", "touch");
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(tooltip.value).not.toBeNull();
    vi.advanceTimersByTime(TOUCH_SHOW_MS);
    expect(tooltip.value).toBeNull();

    // scrolling the list, not pressing
    pointer(button, "pointerdown", "touch", { x: 0, y: 0 });
    pointer(button, "pointermove", "touch", { x: 0, y: 30 });
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(tooltip.value).toBeNull();
    wrapper.unmount();
  });

  it("follows the text, and shows nothing for an empty one", async () => {
    const wrapper = mountButton();
    const button = wrapper.get("button").element;
    text.value = "";
    await nextTick();
    expect(button.getAttribute("aria-description")).toBeNull();
    pointer(button, "pointerenter", "mouse");
    vi.advanceTimersByTime(HOVER_MS);
    expect(tooltip.value).toBeNull();
    wrapper.unmount();
  });
});
