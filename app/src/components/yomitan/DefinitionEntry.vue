<template>
  <span v-if="typeof def === 'string'" class="gloss-content">{{ def }}</span>

  <template v-else-if="def && !Array.isArray(def) && typeof def === 'object'">
    <span v-if="def.type === 'text'" class="gloss-content">{{ def.text }}</span>

    <span
      v-else-if="def.type === 'structured-content'"
      class="gloss-content structured-content"
    >
      <StructuredContent
        :node="def.content"
        :dictionary="dictionary"
        :get-asset-src="getAssetSrc"
        :on-anchor-clicked="onAnchorClicked"
      />
    </span>

    <span v-else-if="def.type === 'image'" class="gloss-content">
      <GlossaryImage
        :image="def as unknown as ImageData"
        :dictionary="dictionary"
        :get-asset-src="getAssetSrc"
      />
    </span>
  </template>

  <!-- [uninflected term, [rules]]: this term is an inflection of another -->
  <span v-else-if="Array.isArray(def)" class="gloss-content gloss-deinflection">
    <a
      class="gloss-link"
      @click.prevent="onAnchorClicked?.(queryHref(def[0]))"
      >{{ def[0] }}</a
    >
    <span v-if="def[1]?.length"> ({{ def[1].join(" → ") }})</span>
  </span>
</template>

<script setup lang="ts">
import GlossaryImage, { type ImageData } from "./GlossaryImage.vue";
import StructuredContent from "./StructuredContent";

defineProps<{
  def: string | { type: string; [k: string]: any } | [string, string[]];
  dictionary: string;
  getAssetSrc: (dict: string, path: string) => string;
  onAnchorClicked?: (href: string) => void;
}>();

const queryHref = (term: string) =>
  `?${new URLSearchParams({ query: term, wildcards: "off" })}`;
</script>
