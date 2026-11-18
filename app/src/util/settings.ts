import { ref, watch } from "vue";

/**
 * Search language ("" = all; `ja-JP`, `zh` or `ko-KR`). The only user setting; persisted
 * per device. Chinese is `zh`, for simplified and traditional alike.
 */
const KEY = "cjk-dict.lang";

/** a stored setting from before Chinese was one choice: `zh-CN` is `zh` */
export function normalizeLang(v: string): string {
  return v.split("-")[0] === "zh" ? "zh" : v;
}

function read(): string {
  try {
    return normalizeLang(localStorage.getItem(KEY) ?? "");
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
