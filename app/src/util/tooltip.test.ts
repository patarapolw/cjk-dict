import { mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h, nextTick, ref, withDirectives } from "vue";

import Tooltip from "../components/Tooltip.vue";
import {
  HOVER_MS,
  LEAVE_GRACE_MS,
  LONG_PRESS_MS,
  describedText,
  hideTooltip,
  setPlatformLongPress,
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
  return e;
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
  const wrapper = mount(Comp, { attachTo: document.body });
  return { wrapper, button: wrapper.get("button").element };
}

beforeEach(() => {
  vi.useFakeTimers();
  text.value = "Words containing it";
  clicks.mockClear();
  setPlatformLongPress(false);
});

afterEach(() => {
  hideTooltip();
  vi.useRealTimers();
});

const bubble = () => document.querySelector("[data-tooltip-bubble]");

describe("v-tooltip", () => {
  it("describes the element to screen readers, without a title", async () => {
    const { wrapper, button } = mountButton();
    expect(button.getAttribute("title")).toBeNull();
    // at all times, not only while shown
    expect(describedText(button)).toBe("Words containing it");
    text.value = "Token search";
    await nextTick();
    expect(describedText(button)).toBe("Token search");
    text.value = "";
    await nextTick();
    expect(button.getAttribute("aria-describedby")).toBeNull();
    wrapper.unmount();
  });

  it("shows on a mouse resting on the element, and stays while it is on the tooltip", async () => {
    const { wrapper, button } = mountButton();
    pointer(button, "pointerenter", "mouse");
    vi.advanceTimersByTime(HOVER_MS - 1);
    expect(tooltip.value).toBeNull();
    vi.advanceTimersByTime(1);
    await nextTick();
    expect(bubble()?.textContent?.trim()).toBe("Words containing it");
    // hidden from screen readers, who have the description
    expect(bubble()?.getAttribute("aria-hidden")).toBe("true");

    // hoverable: onto the tooltip within the grace time
    pointer(button, "pointerleave", "mouse");
    vi.advanceTimersByTime(LEAVE_GRACE_MS - 1);
    pointer(bubble()!, "pointerenter", "mouse");
    vi.advanceTimersByTime(LEAVE_GRACE_MS * 10);
    expect(tooltip.value).not.toBeNull();

    // and off it
    pointer(bubble()!, "pointerleave", "mouse");
    vi.advanceTimersByTime(LEAVE_GRACE_MS);
    await nextTick();
    expect(bubble()).toBeNull();
    wrapper.unmount();
  });

  it("hides on Esc", () => {
    const { wrapper, button } = mountButton();
    pointer(button, "pointerenter", "mouse");
    vi.advanceTimersByTime(HOVER_MS);
    expect(tooltip.value).not.toBeNull();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(tooltip.value).toBeNull();
    wrapper.unmount();
  });

  it("shows on the platform's long press, which then isn't a tap", () => {
    setPlatformLongPress(true);
    const { wrapper, button } = mountButton();

    pointer(button, "pointerdown", "touch");
    // no timer of its own: the system's touch-and-hold delay decides
    vi.advanceTimersByTime(LONG_PRESS_MS * 10);
    expect(tooltip.value).toBeNull();
    const menu = pointer(button, "contextmenu", "touch");
    expect(menu.defaultPrevented).toBe(true);
    expect(tooltip.value?.text).toBe("Words containing it");
    pointer(button, "pointerup", "touch");
    button.click();
    expect(clicks).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("times a long press where the webview doesn't report one", () => {
    const { wrapper, button } = mountButton();

    pointer(button, "pointerdown", "touch");
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(tooltip.value?.text).toBe("Words containing it");
    pointer(button, "pointerup", "touch");
    button.click();
    expect(clicks).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("keeps a long press's tooltip until a touch elsewhere, and a tap is still a tap", () => {
    const { wrapper, button } = mountButton();
    pointer(button, "pointerdown", "touch");
    vi.advanceTimersByTime(LONG_PRESS_MS);
    pointer(button, "pointerup", "touch");
    button.click();

    // persistent: no timer hides it
    vi.advanceTimersByTime(60_000);
    expect(tooltip.value).not.toBeNull();
    pointer(wrapper.get("p").element, "pointerdown", "touch");
    expect(tooltip.value).toBeNull();

    pointer(button, "pointerdown", "touch");
    vi.advanceTimersByTime(100);
    pointer(button, "pointerup", "touch");
    button.click();
    expect(clicks).toHaveBeenCalledTimes(1);
    expect(tooltip.value).toBeNull();
    wrapper.unmount();
  });

  it("shows nothing for a swipe", () => {
    const { wrapper, button } = mountButton();
    pointer(button, "pointerdown", "touch", { x: 0, y: 0 });
    pointer(button, "pointermove", "touch", { x: 0, y: 30 });
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(tooltip.value).toBeNull();
    wrapper.unmount();
  });

  it("shows nothing for an empty text", async () => {
    const { wrapper, button } = mountButton();
    text.value = "";
    await nextTick();
    pointer(button, "pointerenter", "mouse");
    vi.advanceTimersByTime(HOVER_MS);
    expect(tooltip.value).toBeNull();
    wrapper.unmount();
  });
});
