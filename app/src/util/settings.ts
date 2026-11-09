import { ref, watch } from "vue";

/** Search language ("" = all). The only user setting; persisted per device. */
const KEY = "cjk-dict.lang";

function read(): string {
  try {
    return localStorage.getItem(KEY) ?? "";
  } catch {
    return "";
  }
}

export const lang = ref(read());

watch(lang, (v) => {
  try {
    localStorage.setItem(KEY, v);
  } catch {
    // storage unavailable: the choice just isn't remembered
  }
});
