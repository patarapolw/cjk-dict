import { ref } from "vue";

export interface DownloadingProps {
  filename: string;
  outDir?: string;
  url: string;
  /** part of a dictionary sync: committed once all such files are downloaded */
  dict?: boolean;
  /** a font (util/fonts.ts): used as soon as it is downloaded */
  font?: boolean;
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

/** `dict-verify-progress`: checksumming a downloaded dictionary file, or unpacking search.db */
export interface VerifyProgress {
  file: string;
  /** bytes hashed so far (unpacked, when `unpacking`) */
  hashed: number;
  size: number;
  /** unpacking search.db.zst rather than checking a file */
  unpacking?: boolean;
}

/** `Verifying CC-CEDICT.zip`, or `Unpacking search.db` */
export function verifyLabel(v: VerifyProgress): string {
  return `${v.unpacking ? "Unpacking" : "Verifying"} ${v.file}`;
}

export const toBeLoaded = ref<DownloadingProps[]>([]);

/** The file being verified or unpacked, while `dict_commit` checks the library. */
export const verifying = ref<VerifyProgress | null>(null);

/** Downloads that failed this session (retried on next launch). */
export const loadErrors = ref<{ filename: string; error: string }[]>([]);

/** What a dictionary sync is doing: on Android, a notification keeps the app running. */
export interface SyncStatus {
  text: string;
  /** null: indeterminate */
  percent: number | null;
}

/** The sync's status, or null when nothing is syncing. */
export function syncStatus(s: {
  /** `dict_plan` or `dict_commit` in flight */
  checking: boolean;
  verifying: VerifyProgress | null;
  /** the latest progress of the download in flight */
  download: DownloadUrlProgress | null;
  /** downloads not finished yet, the current one first */
  queue: DownloadingProps[];
}): SyncStatus | null {
  const percent = (done: number, total: number) =>
    total > 0 ? Math.min(100, Math.floor((100 * done) / total)) : null;

  if (s.verifying) {
    const { hashed, size } = s.verifying;
    return { text: verifyLabel(s.verifying), percent: percent(hashed, size) };
  }
  if (s.queue.length) {
    const current = s.queue[0];
    const name = current.filename.split("/").pop();
    const left = s.queue.length > 1 ? ` (${s.queue.length - 1} more)` : "";
    const download =
      s.download && s.download.filepath.endsWith(current.filename)
        ? s.download
        : null;
    return {
      text: `Downloading ${name}${left}`,
      percent: download
        ? percent(download.downloaded, download.contentLength)
        : null,
    };
  }
  if (s.checking) return { text: "Checking for updates", percent: null };
  return null;
}

/** Bumped after downloaded dictionaries are put in place, so views can search again. */
export const dictsVersion = ref(0);
