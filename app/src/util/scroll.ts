/**
 * The element that scrolls `el`: `el` itself, or the nearest ancestor that scrolls.
 *
 * In a short window, such as an Android pop-up (floating) window, the whole page
 * scrolls rather than only the results below the search box (see the
 * `max-height` media queries in App.vue and the views), so a view's results list doesn't always scroll itself.
 */
export function scrollContainer(el: HTMLElement): HTMLElement {
  for (let e: HTMLElement | null = el; e; e = e.parentElement) {
    const { overflowY } = getComputedStyle(e);
    if (overflowY === "auto" || overflowY === "scroll") return e;
  }
  return (document.scrollingElement ?? document.documentElement) as HTMLElement;
}

/** Scroll back to the start of `el`, if it has been scrolled past. */
export function scrollToStart(el: HTMLElement) {
  const container = scrollContainer(el);
  if (container === el) {
    el.scrollTop = 0;
    return;
  }
  const above =
    el.getBoundingClientRect().top - container.getBoundingClientRect().top;
  if (above < 0) container.scrollTop += above;
}

/** Pixels left to scroll below the visible part of `el`'s scroll container. */
export function remainingScroll(el: HTMLElement) {
  const c = scrollContainer(el);
  return {
    remaining: c.scrollHeight - c.scrollTop - c.clientHeight,
    clientHeight: c.clientHeight,
  };
}
