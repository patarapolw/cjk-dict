import { invoke } from "@tauri-apps/api/core";
import { watch } from "vue";

import { dictsVersion } from "./loading";

/**
 * Dictionaries may ship a `styles.css` for their structured content (Jitendex, for example,
 * styles `[data-sc-content=...]` blocks). As Yomitan does, each one is nested under
 * `[data-dictionary="<name>"]` so it only applies to that dictionary's entries.
 */
const loaded = new Map<string, Promise<void>>();
/** bumped when the dictionaries change, so a load started before that is dropped */
let generation = 0;

export function loadDictStyles(dict: string): void {
  if (!loaded.has(dict)) loaded.set(dict, load(dict));
}

async function load(dict: string) {
  const gen = generation;
  try {
    const css = await invoke<string | null>("dict_styles", { dict });
    if (!css?.trim() || gen !== generation) return;

    const el = document.createElement("style");
    el.dataset.dictStyles = dict;
    el.textContent = `[data-dictionary="${CSS.escape(dict)}"] {\n${css}\n}`;
    document.head.appendChild(el);
  } catch (e) {
    console.warn(`styles.css of ${dict}`, e);
    if (gen === generation) loaded.delete(dict); // try again with the next entry
  }
}

// a dictionary may have been replaced by a new revision: reload its styles when shown again
watch(dictsVersion, () => {
  generation++;
  loaded.clear();
  for (const el of document.head.querySelectorAll("style[data-dict-styles]")) {
    el.remove();
  }
});
