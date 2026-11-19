<!-- The one tooltip bubble, for v-tooltip (util/tooltip.ts). Screen readers get its text
     from the element's aria-describedby instead, so the bubble itself is hidden from them
     and isn't read twice. -->
<template>
  <div
    v-if="tooltip"
    ref="bubble"
    class="tooltip"
    data-tooltip-bubble
    aria-hidden="true"
    :style="style"
    :data-placement="placement"
    @pointerenter="onPointerEnter"
    @pointerleave="onPointerLeave"
  >
    {{ tooltip.text }}
  </div>
</template>

<script setup lang="ts">
import { nextTick, ref, useTemplateRef, watch } from "vue";

import { keepTooltip, leaveTooltip, tooltip } from "../util/tooltip";

/** space kept from the element and from the window's edges, in px */
const GAP = 6;
const MARGIN = 8;

const elBubble = useTemplateRef("bubble");
const style = ref<Record<string, string>>({});
const placement = ref<"above" | "below">("above");

// hoverable (WCAG 1.4.13): the pointer may move onto the tooltip, e.g. to magnify it
function onPointerEnter(e: PointerEvent) {
  if (e.pointerType !== "touch") keepTooltip();
}
function onPointerLeave(e: PointerEvent) {
  if (e.pointerType !== "touch") leaveTooltip();
}

// above the element, as a finger would cover what's below it; below if there's no room.
// Centred on it, but kept within the window.
watch(tooltip, async (t) => {
  if (!t) return;
  // measure where it can't be seen
  style.value = { visibility: "hidden", left: "0px", top: "0px" };
  await nextTick();
  const bubble = elBubble.value;
  if (!bubble || tooltip.value !== t) return;
  const { width, height } = bubble.getBoundingClientRect();
  const r = t.rect;
  const above = r.top - GAP - height >= MARGIN;
  placement.value = above ? "above" : "below";
  const top = above ? r.top - GAP - height : r.bottom + GAP;
  const maxLeft = window.innerWidth - MARGIN - width;
  const left = Math.max(
    MARGIN,
    Math.min(r.left + r.width / 2 - width / 2, maxLeft),
  );
  style.value = { left: `${left}px`, top: `${top}px` };
});
</script>

<style scoped>
.tooltip {
  position: fixed;
  z-index: 1000;
  max-width: min(20rem, calc(100vw - 16px));
  padding: 0.375rem 0.625rem;
  border-radius: 0.375rem;
  background: var(--app-text);
  color: var(--app-background);
  font-size: 0.875rem;
  font-weight: normal;
  line-height: 1.35;
  white-space: pre-line;
  overflow-wrap: anywhere;
  box-shadow: 0 2px 8px rgb(0 0 0 / 0.25);
  /* its text can be selected and magnified */
  user-select: text;
}
</style>
