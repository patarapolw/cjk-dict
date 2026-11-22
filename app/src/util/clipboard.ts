import { ref } from "vue";

/**
 * The Words view's clipboard monitor, kept outside the view: going to the Characters
 * view unmounts it, and coming back must not turn the monitor off.
 */
export const clipboardMonitorOn = ref(false);

/**
 * The last copied text searched, so coming back doesn't search the same clip again
 * over the search being returned to.
 */
export const lastClipboardText = ref("");
