<template>
  <!-- one dictionary needs no choosing, unless it's left out (from another search
       language's list) and so must be ticked again -->
  <details
    v-if="dicts.length > 1 || excludedCount"
    class="dict-filter"
    :class="{ 'dict-filter-all': !excludedCount }"
  >
    <summary>
      Dictionaries
      <span class="dict-filter-count"
        >({{ includedCount }} of {{ dicts.length }})</span
      >
    </summary>
    <div class="dict-filter-list">
      <label
        v-for="d in dicts"
        :key="d.name"
        v-tooltip="[d.name, d.lang, d.target_lang].filter(Boolean).join(' · ')"
      >
        <!-- the last one ticked can't be unticked: a search needs a dictionary -->
        <input
          type="checkbox"
          :checked="!excluded.includes(d.name)"
          :disabled="includedCount === 1 && !excluded.includes(d.name)"
          @change="
            setIncluded(d.name, ($event.target as HTMLInputElement).checked)
          "
        />
        <!-- a name in Han characters takes its dictionary's forms -->
        <span :lang="d.lang || undefined">{{ d.title || d.name }}</span>
      </label>
    </div>
    <button
      v-if="excludedCount"
      type="button"
      class="link-button"
      @click="includeAll"
    >
      {{ allLabel }}
    </button>
  </details>
  <p
    v-if="noneIncluded(dicts, excluded)"
    class="dict-filter-warning"
    role="alert"
  >
    No dictionary of this language is chosen: tick one in Dictionaries to
    {{ purpose }}.
  </p>
</template>

<script setup lang="ts">
import { computed } from "vue";

import { type IDictInfo, noneIncluded } from "../util/dicts";
import { vTooltip } from "../util/tooltip";

/**
 * The dictionaries to choose from (`dicts`, those of the search language), each ticked
 * unless `excluded` has its name. Dictionaries of other languages in `excluded` stay as
 * they are.
 */
const props = defineProps<{
  dicts: IDictInfo[];
  excluded: string[];
  /** what the button that ticks them all says, e.g. "Search all" */
  allLabel: string;
  /** what a dictionary is needed for, e.g. "search" */
  purpose: string;
}>();

const emit = defineEmits<{ "update:excluded": [names: string[]] }>();

const excludedCount = computed(
  () => props.dicts.filter((d) => props.excluded.includes(d.name)).length,
);
const includedCount = computed(() => props.dicts.length - excludedCount.value);

function setIncluded(name: string, included: boolean) {
  const rest = props.excluded.filter((n) => n !== name);
  emit("update:excluded", included ? rest : [...rest, name]);
}

/** tick the dictionaries shown; those of other languages stay as they are */
function includeAll() {
  const shown = new Set(props.dicts.map((d) => d.name));
  emit(
    "update:excluded",
    props.excluded.filter((n) => !shown.has(n)),
  );
}
</script>

<style scoped>
.dict-filter label {
  display: inline-flex;
  align-items: center;
  gap: 0.5em;
  min-width: 0;
}

.dict-filter summary {
  cursor: pointer;
}

/* the list takes a line of its own when open */
.dict-filter[open] {
  flex-basis: 100%;
}

.dict-filter-count {
  color: var(--app-text-muted);
}

/* collapsed, the count shows only when some are left out */
.dict-filter-all:not([open]) .dict-filter-count {
  display: none;
}

.dict-filter-warning {
  flex-basis: 100%;
  margin: 0;
  color: var(--app-error);
}

.link-button {
  border: 0;
  padding: 0;
  background: transparent;
  color: var(--app-link);
  font: inherit;
  cursor: pointer;
}

.link-button:hover {
  text-decoration: underline;
}

.dict-filter-list {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25em 1em;
  margin: 0.25em 0;
  /* a long list scrolls instead of pushing the results off a short window */
  max-height: 10em;
  overflow: auto;
}
</style>
