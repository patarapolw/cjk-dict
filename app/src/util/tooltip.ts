import { type Directive, shallowRef } from "vue";

/**
 * Tooltips that work with a mouse, a keyboard and a touchscreen, in place of `title`,
 * which a touchscreen never shows. `v-tooltip="text"` on an element; the one bubble is
 * drawn by `components/Tooltip.vue`.
 *
 * Following WCAG 2.1 SC 1.4.13 (Content on Hover or Focus) and the WAI-ARIA Authoring
 * Practices' tooltip pattern:
 *
 * - mouse or pen: shown after resting on the element for {@link HOVER_MS} (a convention,
 *   about as long as a browser's own `title`; no guideline sets it). It stays while the
 *   pointer is on the element or on the tooltip itself (hoverable), so a screen magnifier
 *   can pan over it, with {@link LEAVE_GRACE_MS} to cross the gap between them;
 * - keyboard: shown while the element has keyboard focus;
 * - touch: shown by a long press, which then doesn't count as a tap. The long press is the
 *   platform's own where the webview reports it as `contextmenu` (Android, Windows), so it
 *   follows the system's touch-and-hold delay (Android: Accessibility > Touch & hold
 *   delay). Elsewhere (WebKit on iOS doesn't), a timer of {@link LONG_PRESS_MS} stands in;
 * - it stays until dismissed (persistent): the pointer leaving, focus moving, Esc, the
 *   next touch elsewhere, or a scroll. Nothing hides it on a timer.
 * - screen readers read the text as the element's description (`aria-describedby`), at
 *   all times: a screen reader's cursor neither hovers nor focuses.
 */

/** how long a mouse rests on an element before its tooltip shows */
export const HOVER_MS = 400;
/** the time to move the pointer from an element to its tooltip, or back */
export const LEAVE_GRACE_MS = 150;
/**
 * a long press, where the webview doesn't report the platform's: about the default of
 * iOS and Android
 */
export const LONG_PRESS_MS = 500;
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
let showTimer: ReturnType<typeof setTimeout> | undefined;
let hideTimer: ReturnType<typeof setTimeout> | undefined;
/** a long press has just shown a tooltip: the click that follows isn't a tap */
let swallowClick: HTMLElement | null = null;
/**
 * The webview reports a long press as `contextmenu`: wait for it, at the system's delay,
 * rather than time one. Android's and Windows' webviews do, from the first press; on
 * others, a long press that brings one learns it.
 */
let platformLongPress =
  typeof navigator !== "undefined" &&
  /Android|Windows/.test(navigator.userAgent);

export function showTooltip(el: HTMLElement) {
  const text = texts.get(el);
  clearTimeout(showTimer);
  clearTimeout(hideTimer);
  if (!text || !el.isConnected) return;
  owner = el;
  tooltip.value = { text, rect: el.getBoundingClientRect() };
}

export function hideTooltip() {
  clearTimeout(showTimer);
  removeTooltip();
}

/** hide the tooltip shown, but not one about to show */
function removeTooltip() {
  clearTimeout(hideTimer);
  owner = null;
  tooltip.value = null;
}

/** The pointer left the element or the tooltip: hide, unless it comes onto the other. */
export function leaveTooltip() {
  clearTimeout(hideTimer);
  // not hideTooltip: the pointer may have moved onto another element, whose tooltip is
  // still to show
  if (owner) hideTimer = setTimeout(removeTooltip, LEAVE_GRACE_MS);
}

/** The pointer is on the tooltip (or back on its element): keep it. */
export function keepTooltip() {
  clearTimeout(hideTimer);
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

// --- descriptions for screen readers: a hidden element per tooltip, which the element's
// aria-describedby names. A hidden element still gives its text to a description.

let descriptions: HTMLElement | null = null;
let nextId = 0;
const descIds = new WeakMap<HTMLElement, string>();

function describe(el: HTMLElement, text: string | null | undefined) {
  let id = descIds.get(el);
  if (!text) {
    if (id) undescribe(el);
    return;
  }
  if (!descriptions || !descriptions.isConnected) {
    descriptions = document.createElement("div");
    descriptions.hidden = true;
    descriptions.dataset.tooltipDescriptions = "";
    document.body.append(descriptions);
  }
  let node = id ? document.getElementById(id) : null;
  if (!id || !node) {
    id = `tooltip-${++nextId}`;
    descIds.set(el, id);
    node = document.createElement("span");
    node.id = id;
    descriptions.append(node);
  }
  node.textContent = text;
  const ids = (el.getAttribute("aria-describedby") ?? "")
    .split(/\s+/)
    .filter(Boolean);
  if (!ids.includes(id)) {
    el.setAttribute("aria-describedby", [...ids, id].join(" "));
  }
}

function undescribe(el: HTMLElement) {
  const id = descIds.get(el);
  if (!id) return;
  descIds.delete(el);
  document.getElementById(id)?.remove();
  const ids = (el.getAttribute("aria-describedby") ?? "")
    .split(/\s+/)
    .filter((i) => i && i !== id);
  if (ids.length) el.setAttribute("aria-describedby", ids.join(" "));
  else el.removeAttribute("aria-describedby");
}

/** The tooltip's text, as a screen reader gets it: for tests and debugging. */
export function describedText(el: Element): string {
  return (el.getAttribute("aria-describedby") ?? "")
    .split(/\s+/)
    .map((id) => (id ? document.getElementById(id)?.textContent : ""))
    .filter(Boolean)
    .join(" ");
}

// ---

let globalListeners = false;

/** hide on the next touch elsewhere, scroll or Esc; swallow a long press's click */
function addGlobalListeners() {
  if (globalListeners) return;
  globalListeners = true;
  document.addEventListener(
    "pointerdown",
    (e) => {
      const target = e.target as Element;
      if (
        owner &&
        !owner.contains(target) &&
        !target.closest?.("[data-tooltip-bubble]")
      ) {
        hideTooltip();
      }
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
  /** where a touch on the element started, while it may still become a long press */
  let start: { x: number; y: number } | null = null;

  const longPress = () => {
    start = null;
    clearTimeout(showTimer);
    swallowClick = el;
    showTooltip(el);
  };

  const onPointerDown = (e: PointerEvent) => {
    if (e.pointerType !== "touch") return;
    clearTimeout(showTimer);
    swallowClick = null;
    start = { x: e.clientX, y: e.clientY };
    if (!platformLongPress) showTimer = setTimeout(longPress, LONG_PRESS_MS);
  };
  const onPointerMove = (e: PointerEvent) => {
    if (!start) return;
    if (
      Math.abs(e.clientX - start.x) > MOVE_TOLERANCE ||
      Math.abs(e.clientY - start.y) > MOVE_TOLERANCE
    ) {
      start = null;
      clearTimeout(showTimer);
    }
  };
  const onPointerEnd = () => {
    if (!start) return;
    start = null;
    clearTimeout(showTimer);
  };
  // the platform's long press, at the system's touch-and-hold delay
  const onContextMenu = (e: Event) => {
    const fromTouch =
      start !== null || (e as PointerEvent).pointerType === "touch";
    if (fromTouch) {
      platformLongPress = true;
      e.preventDefault(); // not the system's menu (text selection, "open link")
      if (owner !== el) longPress();
    } else if (swallowClick === el) {
      e.preventDefault();
    }
  };
  const onPointerEnter = (e: PointerEvent) => {
    if (e.pointerType === "touch") return;
    if (owner === el) {
      keepTooltip();
      return;
    }
    clearTimeout(showTimer);
    showTimer = setTimeout(() => showTooltip(el), HOVER_MS);
  };
  const onPointerLeave = (e: PointerEvent) => {
    if (e.pointerType === "touch") return;
    if (owner === el) leaveTooltip();
    else clearTimeout(showTimer);
  };
  const onFocus = () => {
    // a keyboard's focus, not a click's
    if (focusVisible(el)) showTooltip(el);
  };
  const onBlur = () => {
    if (owner === el) hideTooltip();
  };

  const listeners: [string, (e: never) => void][] = [
    ["pointerdown", onPointerDown],
    ["pointermove", onPointerMove],
    ["pointerup", onPointerEnd],
    ["pointercancel", onPointerEnd],
    ["contextmenu", onContextMenu],
    ["pointerenter", onPointerEnter],
    ["pointerleave", onPointerLeave],
    ["focus", onFocus],
    ["blur", onBlur],
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
    el.classList.add("has-tooltip");
  } else {
    texts.delete(el);
    el.classList.remove("has-tooltip");
  }
  describe(el, text);
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
    if (swallowClick === el) swallowClick = null;
    cleanups.get(el)?.();
    cleanups.delete(el);
    texts.delete(el);
    undescribe(el);
  },
};

/** for tests: as on a platform whose webview reports long presses, or not */
export function setPlatformLongPress(value: boolean) {
  platformLongPress = value;
}
