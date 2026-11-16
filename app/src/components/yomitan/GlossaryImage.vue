<template>
  <a
    class="gloss-image-link"
    :data-image-rendering="imageRendering"
    :data-appearance="appearance"
    :data-background="String(image.background ?? true)"
    :data-collapsed="String(collapsed)"
    :data-vertical-align="image.verticalAlign"
    :data-load-state="loadState"
    @click.prevent="toggle"
  >
    <span class="gloss-image-container" :style="containerStyle" :title="title">
      <span
        v-if="appearance === 'monochrome'"
        class="gloss-image-background"
        :style="{ '--image': `url(&quot;${src}&quot;)` }"
      ></span>
      <img
        class="gloss-image"
        :src="src"
        :alt="image.alt ?? title ?? ''"
        loading="lazy"
        @load="onLoad"
        @error="loadState = 'error'"
      />
    </span>
    <span class="gloss-image-link-text">Image</span>
  </a>
  <span v-if="description" class="gloss-image-description">{{
    description
  }}</span>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";

/** Yomitan `img` structured content / `image` glossary, as the dictionary has it */
export interface ImageData {
  path: string;
  /** preferred display size, in px or `sizeUnits` */
  width?: number;
  height?: number;
  title?: string;
  alt?: string;
  description?: string;
  pixelated?: boolean;
  imageRendering?: string;
  appearance?: string;
  background?: boolean;
  collapsed?: boolean;
  collapsible?: boolean;
  verticalAlign?: string;
  border?: string;
  borderRadius?: string;
  sizeUnits?: string;
}

const props = defineProps<{
  image: ImageData;
  dictionary: string;
  getAssetSrc: (dict: string, path: string) => string;
}>();

const src = computed(() =>
  props.image.path ? props.getAssetSrc(props.dictionary, props.image.path) : "",
);

const loadState = ref<"loading" | "loaded" | "error">("loading");
const expanded = ref(false);
const collapsed = computed(() => !!props.image.collapsed && !expanded.value);

function toggle() {
  if (props.image.collapsed || props.image.collapsible === true) {
    expanded.value = !expanded.value;
  }
}

const title = computed(() =>
  typeof props.image.title === "string" ? props.image.title : undefined,
);
const description = computed(() =>
  typeof props.image.description === "string"
    ? props.image.description
    : undefined,
);

const imageRendering = computed(() =>
  typeof props.image.imageRendering === "string"
    ? props.image.imageRendering
    : props.image.pixelated
      ? "pixelated"
      : "auto",
);
const appearance = computed(() =>
  typeof props.image.appearance === "string" ? props.image.appearance : "auto",
);

const num = (n: unknown): n is number =>
  typeof n === "number" && Number.isFinite(n) && n > 0;

/** the image file's own size, known once it loads */
const natural = ref<{ width: number; height: number } | null>(null);

function onLoad(e: Event) {
  const img = e.target as HTMLImageElement;
  if (img.naturalWidth && img.naturalHeight) {
    natural.value = { width: img.naturalWidth, height: img.naturalHeight };
  }
  loadState.value = "loaded";
}

// Same sizing as Yomitan. Its importer turns the dictionary's `width`/`height` into
// `preferredWidth`/`preferredHeight` and stores the file's pixel size as `width`/`height`;
// here the data comes straight from the dictionary, so its `width`/`height` are the
// preferred sizes and the pixel size comes from the loaded image. Preferred sizes win, the
// aspect ratio is kept, and sizes are in px unless sizeUnits is "em" (which only applies to
// preferred sizes). Width is capped at the container width.
const containerStyle = computed(() => {
  const {
    width: preferredWidth,
    height: preferredHeight,
    sizeUnits,
    border,
    borderRadius,
  } = props.image;
  // before the image loads, a size the dictionary doesn't give is a guess
  const w = natural.value?.width ?? 100;
  const h = natural.value?.height ?? 100;
  const hasPW = num(preferredWidth);
  const hasPH = num(preferredHeight);
  const invAspect = hasPW && hasPH ? preferredHeight / preferredWidth : h / w;
  const usedWidth = hasPW
    ? preferredWidth
    : hasPH
      ? preferredHeight / invAspect
      : w;
  const unit = sizeUnits === "em" && (hasPW || hasPH) ? "em" : "px";

  const style: Record<string, string> = {
    width: `${usedWidth}${unit}`,
    aspectRatio: `${1} / ${invAspect}`,
  };
  if (typeof border === "string") style.border = border;
  if (typeof borderRadius === "string") style.borderRadius = borderRadius;
  return style;
});
</script>
