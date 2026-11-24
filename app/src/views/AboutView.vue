<template>
  <main class="about">
    <section>
      <h1>CJK Dictionary</h1>
      <p v-if="version">Version {{ version }}</p>
      <p>
        Code under the
        <ExternalLink :url="`${REPO}/blob/main/LICENSE`"
          >MIT License</ExternalLink
        >:
        <ExternalLink :url="REPO">{{
          REPO.replace("https://", "")
        }}</ExternalLink>
      </p>
      <p>
        The dictionaries and data below aren't part of this app's code. Each
        belongs to its authors and keeps its own license and terms; cjk-dict
        isn't affiliated with them.
      </p>
    </section>

    <section>
      <h2>Dictionaries</h2>
      <p v-if="!dictList.length" class="muted">
        No dictionaries downloaded yet.
      </p>
      <article
        v-for="d in dictList"
        :key="d.name"
        class="credit"
        :data-dictionary="d.name"
      >
        <h3>
          {{ d.title || d.name }}
          <span v-if="d.revision" class="muted">{{ d.revision }}</span>
        </h3>
        <dl>
          <template v-if="DEFAULT_CREDITS[d.name]">
            <dt>Source</dt>
            <dd>
              <ExternalLink :url="DEFAULT_CREDITS[d.name].source.url">{{
                DEFAULT_CREDITS[d.name].source.name
              }}</ExternalLink>
            </dd>
            <template v-if="DEFAULT_CREDITS[d.name].conversion">
              <dt>Yomitan conversion</dt>
              <dd>
                <ExternalLink :url="DEFAULT_CREDITS[d.name].conversion!.url">{{
                  DEFAULT_CREDITS[d.name].conversion!.name
                }}</ExternalLink>
              </dd>
            </template>
            <dt>License</dt>
            <dd>
              <ExternalLink :url="DEFAULT_CREDITS[d.name].license.url">{{
                DEFAULT_CREDITS[d.name].license.name
              }}</ExternalLink>
            </dd>
          </template>
          <template v-if="d.credits?.author">
            <dt>Author</dt>
            <dd>{{ d.credits.author }}</dd>
          </template>
          <template v-if="d.credits?.url && !DEFAULT_CREDITS[d.name]">
            <dt>Source</dt>
            <dd>
              <ExternalLink :url="d.credits.url">{{
                d.credits.url
              }}</ExternalLink>
            </dd>
          </template>
        </dl>
        <!-- the credit the source asks for; a default's own wording, else index.json's -->
        <blockquote v-for="(c, i) in creditsOf(d)" :key="i" class="attribution">
          {{ c }}
        </blockquote>
      </article>
    </section>

    <section>
      <h2>Other data</h2>
      <article v-for="c in OTHER_CREDITS" :key="c.what" class="credit">
        <h3>{{ c.what }}</h3>
        <dl>
          <dt>Source</dt>
          <dd>
            <ExternalLink :url="c.source.url">{{ c.source.name }}</ExternalLink>
          </dd>
          <template v-if="c.license">
            <dt>License</dt>
            <dd>
              <ExternalLink :url="c.license.url">{{
                c.license.name
              }}</ExternalLink>
            </dd>
          </template>
        </dl>
      </article>
    </section>
  </main>
</template>

<script setup lang="ts">
import { getVersion } from "@tauri-apps/api/app";
import { onMounted, ref } from "vue";

import ExternalLink from "../components/ExternalLink.vue";
import { DEFAULT_CREDITS, OTHER_CREDITS } from "../util/credits";
import { dictList, type IDictInfo, loadDicts } from "../util/dicts";

const REPO = "https://github.com/patarapolw/cjk-dict";

const version = ref("");

/**
 * The credits a dictionary asks for: a default's own wording (e.g. Kanjium's), and its
 * index.json's attribution where that says something else
 */
function creditsOf(d: IDictInfo): string[] {
  const out: string[] = [];
  const own = DEFAULT_CREDITS[d.name]?.credit;
  if (own) out.push(own);
  const attribution = d.credits?.attribution;
  if (attribution && !(own && attribution.startsWith(own.replace(/\.$/, "")))) {
    out.push(attribution);
  }
  return out;
}

onMounted(async () => {
  loadDicts();
  try {
    version.value = await getVersion();
  } catch (e) {
    console.warn(e);
  }
});
</script>

<style scoped>
.about {
  box-sizing: border-box;
  min-height: 0;
  height: 100%;
  overflow-y: auto;
  padding: 1em;
  line-height: 1.5;
}

h1 {
  margin: 0 0 0.25em;
  font-size: 1.5rem;
}

h2 {
  margin: 1.5em 0 0.5em;
  font-size: 1.25rem;
}

h3 {
  margin: 0 0 0.25em;
  font-size: 1rem;
}

.credit + .credit {
  margin-top: 1em;
}

dl {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: 0 0.75em;
  margin: 0;
}

dt {
  color: var(--app-text-muted);
}

dd {
  margin: 0;
  overflow-wrap: anywhere;
}

.attribution {
  margin: 0.5em 0 0;
  padding-left: 0.75em;
  border-left: 3px solid var(--app-border);
}

.muted {
  color: var(--app-text-muted);
  font-weight: normal;
}
</style>
