import { describe, expect, it } from "vitest";

import { syncStatus } from "./loading";

const idle = { checking: false, verifying: null, download: null, queue: [] };
const zip = (name: string) => ({
  filename: `dicts/incoming/${name}`,
  url: `https://example.com/${name}`,
  dict: true,
});

describe("syncStatus", () => {
  it("is null when nothing syncs", () => {
    expect(syncStatus(idle)).toBeNull();
  });

  it("checks for updates while dict_plan runs", () => {
    expect(syncStatus({ ...idle, checking: true })).toEqual({
      text: "Checking for updates",
      percent: null,
    });
  });

  it("shows verification progress", () => {
    expect(
      syncStatus({
        ...idle,
        checking: true,
        verifying: { file: "search.db", hashed: 290, size: 1000 },
      }),
    ).toEqual({ text: "Verifying search.db", percent: 29 });
  });

  it("shows how far building search.db is", () => {
    expect(
      syncStatus({
        ...idle,
        checking: true,
        indexing: { step: "CC-CEDICT", done: 3, total: 12 },
      }),
    ).toEqual({ text: "Indexing CC-CEDICT", percent: 25 });
    // a step without banks to count
    expect(
      syncStatus({
        ...idle,
        checking: true,
        indexing: { step: "characters", done: 0, total: 0 },
      }),
    ).toEqual({ text: "Indexing characters", percent: null });
  });

  it("shows the current download's progress and what's left", () => {
    expect(
      syncStatus({
        ...idle,
        queue: [zip("a.zip"), zip("b.zip"), zip("c.zip")],
        download: {
          url: "",
          filepath: "/data/user/0/app/dicts/incoming/a.zip",
          downloaded: 50,
          contentLength: 200,
        },
      }),
    ).toEqual({ text: "Downloading a.zip (2 more)", percent: 25 });
  });

  it("ignores progress left over from a previous download", () => {
    expect(
      syncStatus({
        ...idle,
        queue: [zip("b.zip")],
        download: {
          url: "",
          filepath: "/data/user/0/app/dicts/incoming/a.zip",
          downloaded: 200,
          contentLength: 200,
        },
      }),
    ).toEqual({ text: "Downloading b.zip", percent: null });
  });
});
