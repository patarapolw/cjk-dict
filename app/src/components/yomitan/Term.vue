<template>
  <article class="entry" :data-dictionary="row.dict_path">
    <header class="entry-header">
      <ruby
        v-if="showTerm && row.reading && row.reading !== row.term"
        class="entry-term"
        >{{ row.term }}<rt>{{ row.reading }}</rt></ruby
      >
      <span v-else-if="showTerm" class="entry-term">{{ row.term }}</span>
      <span class="tag" data-category="dictionary">{{ row.dict_path }}</span>
      <span v-for="t in tags" :key="t" class="tag">{{ t }}</span>
    </header>

    <ol class="gloss-list" :data-count="definitions.length">
      <li v-for="(def, i) in definitions" :key="i" class="gloss-item">
        <DefinitionEntry
          :def="def"
          :dictionary="row.dict_path"
          :get-asset-src="getAssetSrc"
          :on-anchor-clicked="onAnchorClicked"
        />
      </li>
    </ol>
  </article>
</template>

<script setup lang="ts">
import { computed, watchEffect } from "vue";

import { ITerm } from "../../db/search.ts";
import { loadDictStyles } from "../../util/dict-styles.ts";
import DefinitionEntry from "./DefinitionEntry.vue";

const props = defineProps<{
  row: ITerm;
  /** show the entry's own term and reading (when it differs from the result heading) */
  showTerm?: boolean;
  getAssetSrc: (dict: string, path: string) => string;
  onAnchorClicked?: (href: string) => void;
}>();

watchEffect(() => loadDictStyles(props.row.dict_path));

/** glossary JSON, as the dictionary has it */
const definitions = computed<any[]>(() => {
  try {
    const v = JSON.parse(props.row.glossary);
    return Array.isArray(v) ? v : [v];
  } catch {
    return [props.row.glossary];
  }
});

const tags = computed(() => [
  ...new Set(
    `${props.row.def_tags} ${props.row.tags}`.split(/\s+/).filter(Boolean),
  ),
]);
</script>

<style scoped>
.entry + .entry {
  margin-top: 0.75em;
}

.entry-header {
  margin-bottom: 0.25em;
}

.entry-term {
  margin-inline-end: 0.5em;
}
</style>
