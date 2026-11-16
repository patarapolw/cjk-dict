import { openUrl } from "@tauri-apps/plugin-opener";
import { type PropType, type VNodeChild, defineComponent, h } from "vue";

import GlossaryImage, { type ImageData } from "./GlossaryImage.vue";

/**
 * Renders Yomitan structured content the way Yomitan's StructuredContentGenerator does:
 * only the elements of the schema, `gloss-sc-<tag>` classes, `data` as `data-sc-*`
 * attributes (which dictionaries' styles.css select on), and only the schema's style keys.
 */

type Node = string | Node[] | ({ tag: string } & Record<string, any>);

/** schema style keys -> CSS. Numbers are em, as in Yomitan (only margins accept them). */
const STYLE_KEYS = [
  "fontStyle",
  "fontWeight",
  "fontSize",
  "color",
  "background",
  "backgroundColor",
  "verticalAlign",
  "textAlign",
  "textEmphasis",
  "textShadow",
  "textDecorationStyle",
  "textDecorationColor",
  "borderColor",
  "borderStyle",
  "borderRadius",
  "borderWidth",
  "clipPath",
  "margin",
  "marginTop",
  "marginLeft",
  "marginRight",
  "marginBottom",
  "padding",
  "paddingTop",
  "paddingLeft",
  "paddingRight",
  "paddingBottom",
  "wordBreak",
  "whiteSpace",
  "cursor",
  "listStyleType",
] as const;
const EM_KEYS = new Set([
  "marginTop",
  "marginLeft",
  "marginRight",
  "marginBottom",
]);

function toStyle(s: unknown): Record<string, string> | undefined {
  if (!s || typeof s !== "object") return undefined;
  const src = s as Record<string, unknown>;
  const out: Record<string, string> = {};
  for (const k of STYLE_KEYS) {
    const v = src[k];
    if (typeof v === "string") out[k] = v;
    else if (typeof v === "number" && EM_KEYS.has(k)) out[k] = `${v}em`;
  }
  const deco = src.textDecorationLine;
  if (typeof deco === "string") out.textDecoration = deco;
  else if (Array.isArray(deco)) out.textDecoration = deco.join(" ");
  return out;
}

/** `{ content: "x", subClass: "y" }` -> `data-sc-content="x" data-sc-sub-class="y"` */
function toDataAttrs(data: unknown): Record<string, string> {
  const out: Record<string, string> = {};
  if (!data || typeof data !== "object") return out;
  for (const [k, v] of Object.entries(data)) {
    if (typeof v !== "string" || !k) continue;
    const name = `data-sc-${k[0].toLowerCase()}${k.slice(1).replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
    if (/^[a-z0-9._-]+$/.test(name)) out[name] = v;
  }
  return out;
}

const SIMPLE = new Set(["ruby", "rt", "rp", "thead", "tbody", "tfoot", "tr"]);
const STYLED = new Set([
  "div",
  "span",
  "ol",
  "ul",
  "li",
  "details",
  "summary",
  "th",
  "td",
]);

export default defineComponent({
  name: "StructuredContent",
  props: {
    node: { type: [String, Array, Object] as PropType<Node>, required: true },
    dictionary: { type: String, required: true },
    getAssetSrc: {
      type: Function as PropType<(dict: string, path: string) => string>,
      required: true,
    },
    onAnchorClicked: Function as PropType<(href: string) => void>,
  },
  setup(props) {
    function render(node: unknown): VNodeChild {
      if (typeof node === "string") return node;
      if (Array.isArray(node)) return node.map(render);
      if (!node || typeof node !== "object") return null;

      const el = node as Record<string, any>;
      const tag = el.tag;
      const base = {
        class: `gloss-sc-${tag}`,
        lang: typeof el.lang === "string" ? el.lang : undefined,
        ...toDataAttrs(el.data),
      };

      if (tag === "br") return h("br", base);
      if (tag === "img") {
        return h(GlossaryImage, {
          image: el as ImageData,
          dictionary: props.dictionary,
          getAssetSrc: props.getAssetSrc,
        });
      }
      if (tag === "a") return renderLink(el);
      if (tag === "table") {
        return h("div", { class: "gloss-sc-table-container" }, [
          h("table", base, render(el.content) as any),
        ]);
      }
      if (SIMPLE.has(tag)) return h(tag, base, render(el.content) as any);
      if (STYLED.has(tag)) {
        const attrs: Record<string, unknown> = {
          ...base,
          style: toStyle(el.style),
          title: typeof el.title === "string" ? el.title : undefined,
        };
        if (tag === "td" || tag === "th") {
          if (typeof el.colSpan === "number") attrs.colspan = el.colSpan;
          if (typeof el.rowSpan === "number") attrs.rowspan = el.rowSpan;
        }
        if (tag === "details" && el.open === true) attrs.open = true;
        return h(tag, attrs, render(el.content) as any);
      }
      return null; // not in the schema
    }

    function renderLink(el: Record<string, any>): VNodeChild {
      const href = typeof el.href === "string" ? el.href : "";
      const internal = href.startsWith("?");
      const external = /^https?:\/\//i.test(href);
      return h(
        "a",
        {
          class: "gloss-link",
          href: external ? href : undefined,
          lang: typeof el.lang === "string" ? el.lang : undefined,
          "data-external": String(!internal),
          onClick: (e: MouseEvent) => {
            e.preventDefault();
            if (internal) props.onAnchorClicked?.(href);
            else if (external) openUrl(href);
          },
        },
        [
          h("span", { class: "gloss-link-text" }, render(el.content) as any),
          external
            ? h(
                "span",
                { class: "gloss-link-external-icon", "aria-hidden": "true" },
                "↗",
              )
            : null,
        ],
      );
    }

    return () => render(props.node);
  },
});
