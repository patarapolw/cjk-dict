<template>
  <!-- laid out as Yomitan's pronunciations: a list of dictionaries (numbered if more than
       one), each with its tag and its pitches (a list if more than one) -->
  <ol class="pronunciation-group-list" lang="ja-JP" :data-count="groups.length">
    <li
      v-for="g in groups"
      :key="g.dict_path"
      class="pronunciation-group"
      :data-dictionary="g.dict_path"
    >
      <span class="tag" data-category="pronunciation-dictionary">{{
        g.dict_path
      }}</span>
      <ul class="pronunciation-list" :data-count="g.pitches.length">
        <li
          v-for="({ reading, pitch }, i) in g.pitches"
          :key="i"
          class="pronunciation"
          :data-pitch-accent-downstep-position="downsteps(pitch.position)"
        >
          <span v-for="t in pitch.tags ?? []" :key="t" class="tag">{{
            t
          }}</span>
          <span class="pronunciation-text" v-tooltip="describe(reading, pitch)"
            ><span
              v-for="(m, k) in pitchMorae(reading, pitch)"
              :key="k"
              class="pronunciation-mora"
              :data-pitch="m.high ? 'high' : 'low'"
              :data-pitch-next="m.drop ? 'low' : undefined"
              :data-nasal="m.nasal || undefined"
              :data-devoice="m.devoice || undefined"
              >{{ m.nasal ? withoutDakuten(m.text) : m.text
              }}<span
                v-if="m.nasal"
                class="pronunciation-nasal-indicator" /><span
                v-if="m.devoice"
                class="pronunciation-devoice-indicator" /><span
                class="pronunciation-mora-line" /></span></span
          ><span class="pronunciation-downstep-notation"
            >[{{ downsteps(pitch.position).join(",") }}]</span
          >
        </li>
      </ul>
    </li>
  </ol>
</template>

<script setup lang="ts">
import { computed } from "vue";

import {
  downsteps,
  groupByDictionary,
  IPitch,
  IPitchEntry,
  isVerbOrAdjective,
  pitchDescription,
  pitchMorae,
  withoutDakuten,
} from "../util/pitch";
import { vTooltip } from "../util/tooltip";

const props = defineProps<{
  /** the pitch accent dictionaries' entries for a word */
  entries: IPitchEntry[];
  /** the word's deinflection rules, e.g. `v5`: a verb's drop is kifuku */
  rules?: string | null;
}>();

const groups = computed(() => groupByDictionary(props.entries));

function describe(reading: string, pitch: IPitch): string {
  return pitchDescription(reading, pitch, isVerbOrAdjective(props.rules));
}
</script>

<style scoped>
.pronunciation-group-list {
  margin: 0 0 0.5rem;
  padding: 0 0 0 1.4em;
  list-style-type: decimal;
}

.pronunciation-group-list[data-count="1"] {
  padding-left: 0;
  list-style-type: none;
}

.pronunciation-list {
  margin: 0;
  padding: 0 0 0 1.4em;
  list-style-type: circle;
}

/* one pitch: on the dictionary's line */
.pronunciation-list[data-count="1"] {
  display: inline;
  padding-left: 0;
  list-style-type: none;
}

.pronunciation-list[data-count="1"] .pronunciation {
  display: inline;
}

.pronunciation-group::marker,
.pronunciation::marker {
  color: var(--app-text-muted);
}

.pronunciation {
  line-height: 1.5;
}

.pronunciation-text {
  display: inline;
}

.pronunciation-mora {
  display: inline-block;
  position: relative;
}

/* the line over a high mora; where the pitch drops, it turns down a little */
.pronunciation-mora[data-pitch="high"] > .pronunciation-mora-line {
  position: absolute;
  top: 0.1em;
  left: 0;
  right: 0;
  border-top: 0.1em solid currentColor;
  pointer-events: none;
}

.pronunciation-mora[data-pitch="high"][data-pitch-next="low"] {
  padding-right: 0.1em;
  margin-right: 0.1em;
}

.pronunciation-mora[data-pitch="high"][data-pitch-next="low"]
  > .pronunciation-mora-line {
  right: -0.1em;
  height: 0.4em;
  border-right: 0.1em solid currentColor;
}

/* a nasal mora (鼻濁音): its kana without dakuten, and a small circle */
.pronunciation-nasal-indicator {
  position: absolute;
  top: 0.125em;
  right: -0.125em;
  width: 0.375em;
  height: 0.375em;
  border: 1px solid var(--app-error);
  border-radius: 50%;
  box-sizing: border-box;
}

/* a devoiced mora: a dotted circle around it */
.pronunciation-devoice-indicator {
  position: absolute;
  top: 50%;
  left: 50%;
  width: 1.125em;
  height: 1.125em;
  border: 1px dotted var(--app-error);
  border-radius: 50%;
  box-sizing: border-box;
  transform: translate(-50%, -50%);
}

.pronunciation-downstep-notation {
  margin-left: 0.25em;
}
</style>
