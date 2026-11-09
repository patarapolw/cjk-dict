<template>
  <div>
    <p>
      {{ message }}
      <span v-if="progressMessage" style="float: right">
        ({{ progressMessage }})
      </span>
    </p>
    <ProgressBar
      v-if="isStarted"
      :mode="progressMessage ? 'determinate' : 'indeterminate'"
      :value="percentage"
      :show-value="false"
    />
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";

import ProgressBar from "primevue/progressbar";

import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

import {
  DownloadingProps,
  DownloadUrlProgress,
  UnzipProgress,
} from "../util/loading";

const props = defineProps<DownloadingProps>();

defineExpose({
  start,
});

const isStarted = ref(false);
const message = ref(
  "Waiting to " +
    (props.url
      ? `download ${props.filename} from ${props.url}`
      : `parse ${props.filename}`),
);
const progressMessage = ref("");
const percentage = ref(0);

function formatFileSize(n: number) {
  const units = ["bytes", "KB", "MB"];
  let i = 0;

  while (i++ < units.length) {
    if (n < 1024) break;
    n /= 1024;
  }
  return `${n.toPrecision(3)} ${units[i - 1]}`;
}

const unlisteners = ref<UnlistenFn[]>([]);

async function start() {
  isStarted.value = true;

  // **Downloading block
  const { url, filename, outDir } = props;

  unlisteners.value = [
    ...unlisteners.value,
    await listen<DownloadUrlProgress>(
      "download-url-progress",
      ({ payload }) => {
        message.value = `Downloading ${payload.filepath} from ${payload.url}`;
        progressMessage.value = `${formatFileSize(payload.downloaded)} / ${formatFileSize(payload.contentLength)}`;
        percentage.value = Math.round(
          (100 * payload.downloaded) / payload.contentLength,
        );
      },
    ),
    await listen<UnzipProgress>("unzip-progress", ({ payload }) => {
      message.value = `Unzipping ${payload.filename} to ${payload.outDir}/`;
      progressMessage.value = "";
      percentage.value = 0;
    }),
  ];

  if (props.outDir) {
    await invoke<boolean>("download_and_unzip", {
      url,
      filename,
      outDir,
    });
  } else {
    await invoke<boolean>("download_url", {
      url,
      filename,
    });
  }

  unlisteners.value.map((u) => u());
  unlisteners.value = [];
}

onBeforeUnmount(() => {
  unlisteners.value.map((u) => u());
});
</script>

<style scoped></style>
