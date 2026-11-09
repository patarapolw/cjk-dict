<template>
  <main class="container">
    <fieldset class="fieldset">
      <legend>Dictionary server</legend>
      <label class="row">
        <span class="label-header">URL: </span>
        <InputText
          class="flex-grow"
          v-model="settingsState.state.dictApiURL"
          :placeholder="settingsState.default.dictApiURL || urlExample"
        />
      </label>
      <p class="hint">
        The cjk-dict worker. Dictionaries are managed on the server with
        <code>dict-admin</code>.
      </p>
    </fieldset>
  </main>
</template>

<script setup lang="ts">
import InputText from "primevue/inputtext";

import { settingsState } from "../util/settings";

const urlExample = "https://cjk-dict.<account>.workers.dev";
</script>

<style scoped>
.container {
  padding: 1em;
  overflow: auto;
  height: 100%;
}

.row {
  display: flex;
  align-items: center;
  gap: 0.5em;
}

.flex-grow {
  flex-grow: 1;
}

.hint {
  font-size: 0.875rem;
  opacity: 0.75;
}
</style>
