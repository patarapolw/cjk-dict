import { computed, ComputedRef, reactive, watch } from "vue";

interface ISettingsState {
  /** cjk-dict worker, e.g. https://cjk-dict.<account>.workers.dev */
  dictApiURL?: string;
  lang: string;
}

type ComputedSettingsState = Required<{
  [k in keyof ISettingsState]: ComputedRef<ISettingsState[k]>;
}>;

class SettingsState {
  state = reactive<ISettingsState>({
    lang: "",
  });
  computed: ComputedSettingsState;
  default: ISettingsState = {
    dictApiURL: import.meta.env.VITE_DICT_API_URL,
    lang: "",
  };

  LSKEY = "CJK_DICT_SETTINGS(1)";

  constructor() {
    this.computed = Object.fromEntries(
      Object.keys(this.default).map((k) => [
        k,
        computed(
          () =>
            this.state[k as keyof ISettingsState] ||
            this.default[k as keyof ISettingsState],
        ),
      ]),
    ) as ComputedSettingsState;
  }

  load() {
    try {
      Object.assign(
        this.state,
        JSON.parse(localStorage.getItem(this.LSKEY) || "{}"),
      );
    } catch (e) {
      console.error(e);
    }
    watch(this.state, () => this.save(), { deep: true });
  }

  save() {
    try {
      localStorage.setItem(this.LSKEY, JSON.stringify(this.state));
    } catch (e) {
      console.error(e);
    }
  }
}

export const settingsState = new SettingsState();
