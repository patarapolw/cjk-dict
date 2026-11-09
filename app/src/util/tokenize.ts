import { BaseDirectory, exists } from "@tauri-apps/plugin-fs";

import { DownloadingProps } from "./loading";

export async function makeLinderaDownloadList(
  models: string[],
): Promise<DownloadingProps[]> {
  const version = "6.2.0";

  const out: DownloadingProps[] = [];

  for (const model of models) {
    if (
      await exists(`lindera/lindera-${model}`, {
        baseDir: BaseDirectory.AppData,
      })
    )
      continue;

    const filename = `lindera-${model}-${version}.zip`;
    const url = `https://github.com/lindera/lindera/releases/download/v${version}/${filename}`;

    out.push({
      filename,
      outDir: "lindera",
      url,
    });
  }

  return out;
}
