<template>
  <!-- 1. Text node -->
  <template v-if="typeof node === 'string'">{{ node }}</template>

  <!-- 2. Array of child nodes -->
  <template v-else-if="Array.isArray(node)">
    <StructuredContent
      v-for="(child, i) in node"
      :key="i"
      :node="child"
      :dictionary="dictionary"
      :get-asset-src="getAssetSrc"
      :on-anchor-clicked="onAnchorClicked"
    />
  </template>

  <!-- 3. Element object -->
  <template v-else-if="node && node.tag">
    <!-- Void element: <br> -->
    <br v-if="node.tag === 'br'" v-bind="dataAttrs" />

    <!-- Image -->
    <img v-else-if="node.tag === 'img'" v-bind="imgAttrs" />

    <!-- Anchor -->
    <a
      v-else-if="node.tag === 'a'"
      :href="node.href"
      :lang="node.lang"
      v-bind="safeDataAttrs"
      @click="handleAnchorClick"
    >
      <StructuredContent
        v-if="safeContent !== undefined"
        :node="safeContent"
        :dictionary="dictionary"
        :get-asset-src="getAssetSrc"
        :on-anchor-clicked="onAnchorClicked"
      />
    </a>

    <!-- Everything else -->
    <component
      v-else
      :is="node.tag"
      :lang="node.lang"
      :title="node.title"
      :colspan="node.colSpan"
      :rowspan="node.rowSpan"
      :open="node.open"
      :style="elementStyle"
      v-bind="dataAttrs"
    >
      <StructuredContent
        :node="node.content"
        :dictionary="dictionary"
        :get-asset-src="getAssetSrc"
        :on-anchor-clicked="onAnchorClicked"
      />
    </component>
  </template>
</template>

<script setup lang="ts">
import { openUrl } from "@tauri-apps/plugin-opener";
import { computed } from "vue";

type StructuredContentNode =
  | string
  | StructuredContentNode[]
  | (Record<string, any> & { tag: string });

const props = defineProps<{
  node: StructuredContentNode;
  dictionary: string;
  getAssetSrc: (dict: string, path: string) => string;
  onAnchorClicked?: (src: string) => void;
}>();

const dataAttrs = computed(() => {
  const data = (props.node as any)?.data ?? {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(data)) {
    out[k.startsWith("data-") ? k : `data-${k}`] = v as string;
  }
  return out;
});

// Only pass through `data-*` keys. Anything else is a dictionary-author mistake
// or a collision with a real attribute (href, style, on*), and must not leak.
const safeDataAttrs = computed(() => {
  const data = (props.node as any)?.data ?? {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(data)) {
    if (typeof v !== "string") continue;
    if (k.startsWith("data-")) out[k] = v;
    // else: drop it. Do NOT auto-prefix, because that's how `href` became
    // a second, attribute-less <a> in the first place.
  }
  return out;
});

// Guard against self-referential or duplicate content.
const safeContent = computed(() => {
  const node = props.node as any;
  if (!node || typeof node !== "object") return undefined;
  const content = node.content;
  if (content === undefined || content === null) return undefined;
  // Self-reference
  if (content === node) return undefined;
  // Array containing the node itself
  if (Array.isArray(content) && content.some((c) => c === node)) {
    return content.filter((c) => c !== node);
  }
  return content;
});

const imgAttrs = computed(() => {
  const node = props.node as any;
  if (!node || node.tag !== "img") return {};

  const attrs: Record<string, any> = {
    src: resolvedSrc.value,
  };

  if (node.title) attrs.title = node.title;
  if (node.alt) attrs.alt = node.alt;

  // --- width / height ---
  const units: "px" | "em" = node.sizeUnits === "em" ? "em" : "px";
  const isValid = (n: unknown): n is number =>
    typeof n === "number" && Number.isFinite(n) && n > 1;

  const style: Record<string, any> = { ...(imageStyle.value ?? {}) };

  if (units === "em") {
    if (isValid(node.width)) style.width = `${node.width}em`;
    if (isValid(node.height)) style.height = `${node.height}em`;
  } else {
    if (isValid(node.width)) attrs.width = node.width;
    if (isValid(node.height)) attrs.height = node.height;
  }

  if (Object.keys(style).length) attrs.style = style;

  // --- data-* attrs (merged in) ---
  Object.assign(attrs, safeDataAttrs.value);

  return attrs;
});

const elementStyle = computed(() => {
  const s = (props.node as any)?.style;
  if (!s) return undefined;
  const style: Record<string, any> = { ...s };
  if (Array.isArray(style.textDecorationLine)) {
    style.textDecorationLine = style.textDecorationLine.join(" ");
  }
  return style;
});

const imageStyle = computed(() => {
  const node = props.node as any;
  const style: Record<string, any> = {};
  if (node.border) style.border = node.border;
  if (node.borderRadius) style.borderRadius = node.borderRadius;
  if (node.verticalAlign) style.verticalAlign = node.verticalAlign;
  if (node.pixelated || node.imageRendering) {
    style.imageRendering =
      node.imageRendering ?? (node.pixelated ? "pixelated" : "auto");
  }
  return style;
});

const resolvedSrc = computed(() => {
  const node = props.node as any;
  return node && node.tag === "img" && node.path
    ? props.getAssetSrc(props.dictionary, node.path)
    : "";
});

function handleAnchorClick(e: MouseEvent) {
  e.preventDefault();

  const node = props.node as any;
  const href: string | undefined = node?.href;
  if (!href) return;

  // Schema: URLs starting with `?` are internal dictionary links.
  if (href.startsWith("?")) {
    e.preventDefault();
    props.onAnchorClicked?.(href);
    return;
  }

  // External http(s) link: let the browser handle it, but don't let the
  // click bubble up to a parent anchor/handler if you don't want that.
  // (Remove this if you want default bubble behavior.)
  // e.stopPropagation()
  openUrl(href);
}
</script>
