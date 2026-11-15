<template>
  <div>
    <p>
      {{ message }}
      <span v-if="progressMessage" style="float: right">
        ({{ progressMessage }})
      </span>
    </p>
    <div
      v-if="isStarted"
      class="progress"
      :class="{ indeterminate: !progressMessage }"
      role="progressbar"
      :aria-label="message"
      aria-valuemin="0"
      aria-valuemax="100"
      :aria-valuenow="progressMessage ? percentage : undefined"
    >
      <div
        class="progress-value"
        :style="{ width: progressMessage ? `${percentage}%` : undefined }"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";

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
      ? `download ${props.filename.split("/").pop()}`
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
        message.value = `Downloading ${payload.filepath.split("/").pop()}`;
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

<style scoped>
.progress {
  height: 0.5rem;
  overflow: hidden;
  border-radius: 999px;
  background: #e5e7eb;
}

.progress-value {
  height: 100%;
  background: #2563eb;
  transition: width 150ms ease;
}

.progress.indeterminate .progress-value {
  width: 40%;
  animation: progress 1.2s ease-in-out infinite alternate;
}

@keyframes progress {
  from {
    transform: translateX(-100%);
  }
  to {
    transform: translateX(250%);
  }
}
</style>
