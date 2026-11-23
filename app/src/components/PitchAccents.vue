<template>
  <ul class="pitches" lang="ja-JP">
    <template v-for="(ent, i) in entries" :key="i">
      <li
        v-for="(p, j) in ent.pitches"
        :key="`${i}-${j}`"
        class="pitch"
        :data-dictionary="ent.dict_path"
      >
        <span
          class="pitch-reading"
          :aria-label="ariaLabel(ent.reading, p)"
          v-tooltip="pitchDescription(ent.reading, p.position)"
          ><span
            v-for="(m, k) in pitchMorae(ent.reading, p.position)"
            :key="k"
            class="mora"
            :class="{ high: m.high, drop: m.drop }"
            aria-hidden="true"
            >{{ m.text }}</span
          ></span
        >
        <span v-if="downstep(p.position) !== null" class="downstep"
          >[{{ downstep(p.position) }}]</span
        >
        <span v-for="t in p.tags ?? []" :key="t" class="tag">{{ t }}</span>
        <span class="tag" data-category="dictionary">{{ ent.dict_path }}</span>
      </li>
    </template>
  </ul>
</template>

<script setup lang="ts">
import {
  downstep,
  IPitch,
  IPitchEntry,
  pitchDescription,
  pitchMorae,
} from "../util/pitch";
import { vTooltip } from "../util/tooltip";

defineProps<{
  /** the pitch accent dictionaries' entries for a word */
  entries: IPitchEntry[];
}>();

/** what a screen reader says: the reading, then where the pitch drops */
function ariaLabel(reading: string, p: IPitch): string {
  const n = downstep(p.position);
  return n === null
    ? `${reading}, ${p.position}`
    : `${reading}, ${pitchDescription(reading, p.position)}`;
}
</script>

<style scoped>
.pitches {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem 1rem;
  margin: 0 0 0.5rem;
  padding: 0;
  list-style: none;
}

.pitch {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0.25rem;
}

.pitch-reading {
  /* room above the morae for the high line */
  padding-top: 0.15em;
}

.mora {
  /* the lines are drawn on every mora, transparent where they're not, so the text
     doesn't shift */
  border-top: 2px solid transparent;
  border-right: 2px solid transparent;
}

.mora.high {
  border-top-color: currentColor;
}

/* the pitch drops after this mora: the high line turns down */
.mora.drop {
  border-right-color: currentColor;
}

.downstep {
  font-size: 0.875em;
  opacity: 0.8;
}
</style>
