import { type Directive, shallowRef } from "vue";

/**
 * Tooltips that work with a mouse, a keyboard and a touchscreen, in place of `title`,
 * which a touchscreen never shows. `v-tooltip="text"` on an element; the one bubble is
 * drawn by `components/Tooltip.vue`.
 *
 * - mouse or pen: shown after resting on the element for {@link HOVER_MS};
 * - keyboard: shown while the element has focus;
 * - touch: shown by a long press ({@link LONG_PRESS_MS}), which then doesn't count as a
 *   tap; it stays up for {@link TOUCH_SHOW_MS}, or until the next touch or scroll.
 */

export const HOVER_MS = 400;
export const LONG_PRESS_MS = 500;
export const TOUCH_SHOW_MS = 4000;
/** how far a finger may move during a long press, in px */
const MOVE_TOLERANCE = 10;

export interface TooltipState {
  text: string;
  /** the element's box, to place the bubble by */
  rect: DOMRect;
}

/** the tooltip shown, if any */
export const tooltip = shallowRef<TooltipState | null>(null);

let owner: HTMLElement | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
/** a long press has just shown a tooltip: the click that follows isn't a tap */
let swallowClick: HTMLElement | null = null;

export function showTooltip(el: HTMLElement) {
  const text = texts.get(el);
  if (!text || !el.isConnected) return;
  owner = el;
  tooltip.value = { text, rect: el.getBoundingClientRect() };
}

export function hideTooltip() {
  clearTimeout(timer);
  owner = null;
  tooltip.value = null;
}

function focusVisible(el: HTMLElement) {
  try {
    return el.matches(":focus-visible");
  } catch {
    // a selector the engine doesn't know
    return false;
  }
}

const texts = new WeakMap<HTMLElement, string>();
const cleanups = new WeakMap<HTMLElement, () => void>();

let globalListeners = false;

/** hide on the next touch elsewhere, scroll or Esc; swallow a long press's click */
function addGlobalListeners() {
  if (globalListeners) return;
  globalListeners = true;
  document.addEventListener(
    "pointerdown",
    (e) => {
      if (owner && !owner.contains(e.target as Node)) hideTooltip();
    },
    true,
  );
  document.addEventListener("scroll", () => owner && hideTooltip(), true);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && owner) hideTooltip();
  });
  // capture, on the document: before the element's own click handlers
  document.addEventListener(
    "click",
    (e) => {
      if (swallowClick && swallowClick.contains(e.target as Node)) {
        e.preventDefault();
        e.stopImmediatePropagation();
      }
      swallowClick = null;
    },
    true,
  );
}

function bind(el: HTMLElement) {
  let start: { x: number; y: number } | null = null;

  const onPointerDown = (e: PointerEvent) => {
    if (e.pointerType !== "touch") return;
    clearTimeout(timer);
    swallowClick = null;
    start = { x: e.clientX, y: e.clientY };
    timer = setTimeout(() => {
      start = null;
      swallowClick = el;
      showTooltip(el);
      timer = setTimeout(hideTooltip, TOUCH_SHOW_MS);
    }, LONG_PRESS_MS);
  };
  const onPointerMove = (e: PointerEvent) => {
    if (!start) return;
    if (
      Math.abs(e.clientX - start.x) > MOVE_TOLERANCE ||
      Math.abs(e.clientY - start.y) > MOVE_TOLERANCE
    ) {
      start = null;
      clearTimeout(timer);
    }
  };
  const onPointerEnd = () => {
    if (!start) return;
    start = null;
    clearTimeout(timer);
  };
  const onPointerEnter = (e: PointerEvent) => {
    if (e.pointerType === "touch") return;
    clearTimeout(timer);
    timer = setTimeout(() => showTooltip(el), HOVER_MS);
  };
  const onPointerLeave = (e: PointerEvent) => {
    if (e.pointerType === "touch") return;
    if (owner === el) hideTooltip();
    else clearTimeout(timer);
  };
  const onFocus = () => {
    // a keyboard's focus, not a click's
    if (focusVisible(el)) showTooltip(el);
  };
  const onBlur = () => {
    if (owner === el) hideTooltip();
  };
  // Android's long-press menu (text selection, "open link")
  const onContextMenu = (e: Event) => {
    if (swallowClick === el) e.preventDefault();
  };

  const listeners: [string, (e: never) => void][] = [
    ["pointerdown", onPointerDown],
    ["pointermove", onPointerMove],
    ["pointerup", onPointerEnd],
    ["pointercancel", onPointerEnd],
    ["pointerenter", onPointerEnter],
    ["pointerleave", onPointerLeave],
    ["focus", onFocus],
    ["blur", onBlur],
    ["contextmenu", onContextMenu],
  ];
  for (const [type, fn] of listeners) el.addEventListener(type, fn as never);
  cleanups.set(el, () => {
    for (const [type, fn] of listeners)
      el.removeEventListener(type, fn as never);
  });
}

function setText(el: HTMLElement, text: string | null | undefined) {
  if (text) {
    texts.set(el, text);
    el.setAttribute("aria-description", text);
    el.classList.add("has-tooltip");
  } else {
    texts.delete(el);
    el.removeAttribute("aria-description");
    el.classList.remove("has-tooltip");
  }
  if (owner === el) {
    if (text) showTooltip(el);
    else hideTooltip();
  }
}

/** `v-tooltip="text"`: an empty text shows none */
export const vTooltip: Directive<HTMLElement, string | null | undefined> = {
  mounted(el, { value }) {
    addGlobalListeners();
    bind(el);
    setText(el, value);
  },
  updated(el, { value, oldValue }) {
    if (value !== oldValue) setText(el, value);
  },
  beforeUnmount(el) {
    if (owner === el || swallowClick === el) hideTooltip();
    cleanups.get(el)?.();
    cleanups.delete(el);
    texts.delete(el);
  },
};
