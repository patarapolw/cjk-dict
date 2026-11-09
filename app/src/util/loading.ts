import { ref } from "vue";

export interface DownloadingProps {
  filename: string;
  outDir?: string;
  url: string;
  /** part of a dictionary sync: committed once all such files are downloaded */
  dict?: boolean;
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

/** Bumped after downloaded dictionaries are put in place, so views can search again. */
export const dictsVersion = ref(0);
