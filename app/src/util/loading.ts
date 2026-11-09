import { ref } from "vue";

export interface DownloadingProps {
  filename: string;
  outDir?: string;
  url: string;
}

export interface DownloadUrlProgress {
  url: string;
  filepath: string;
  contentLength: number;
  downloaded: number;
}

export interface UnzipProgress {
  filename: string;
  outDir: string;
}

export const toBeLoaded = ref<DownloadingProps[]>([]);

/** Downloads that failed this session (retried on next launch). */
export const loadErrors = ref<{ filename: string; error: string }[]>([]);
