<template>
  <article class="term">
    <header>
      <ruby v-if="$props.row.reading && $props.row.reading !== $props.row.term">
        {{ $props.row.term }}<rt>{{ $props.row.reading }}</rt>
      </ruby>
      <span v-else>{{ $props.row.term }}</span>
      <span v-if="$props.row.def_tags" class="tags">{{
        $props.row.def_tags
      }}</span>
      <span v-if="$props.row.tags" class="tags">{{ $props.row.tags }}</span>
    </header>

    <ol class="definitions">
      <li v-for="(def, i) in definitions" :key="i">
        <DefinitionEntry
          :def="def"
          :dictionary="dictionary"
          :get-asset-src="getAssetSrc"
          :on-anchor-clicked="onAnchorClicked"
        />
      </li>
    </ol>
  </article>
</template>

<script setup lang="ts">
import { ITerm } from "../../db/search.ts";
import DefinitionEntry from "./DefinitionEntry.vue";

const props = defineProps<{
  row: ITerm;
  dictionary: string;
  getAssetSrc: (dict: string, path: string) => string;
  onAnchorClicked?: (src: string) => void;
}>();

const definitions = JSON.parse(props.row.glossary);
</script>

<style scoped>
.term :deep(img) {
  max-width: 100%;
  height: 1em;
  /* Guard against dictionaries that ship tiny/hidden placeholders */
  min-width: 1em;
  min-height: 1em;
}
</style>
