#!/usr/bin/env node
// Push the dict-admin library to the Android app over adb, so the app finds its files
// already downloaded rather than downloading them from R2.
//
//   node scripts/push-library.mjs        (`pnpm android` runs it before `tauri android run`)
//
// - Files go to the app's `dicts/incoming/`, where downloads land. `dict_plan` keeps a file
//   there whose size matches the manifest, and `dict_commit` checksums it as any download.
// - Then, once every file is there, the library's manifest.json, to `dicts/incoming/` too.
//   The app plans from it only without a remote: none configured (no CJK_DICT_R2_* or
//   CJK_DICT_BASE_URL), or one it can't reach. So the app can be used fully offline.
// - With a remote it reaches, the app plans from the remote's manifest.json, so this helps
//   as far as the library matches what `pnpm admin upload` sent. A file that doesn't is
//   deleted (other size) or downloaded again (checksum mismatch): it costs time, never
//   correctness.
// - Pushes only what the app lacks: not a file its committed manifest vouches for, nor one
//   already in `incoming/` with the right size; not search.db.zst when its search.db is the
//   current one.
// - Needs the app installed (a debug build, for `run-as`): on a first install it pushes
//   nothing, and the app downloads the library.
// - adb picks the device as usual: set ANDROID_SERIAL when several are connected.

import { spawn, spawnSync } from "node:child_process";
import { createReadStream, existsSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

const PKG = "cc.polv.cjk_dict";
const MANIFEST = "manifest.json";
const SEARCH_DB = "search.db";
// a file being pushed; renamed when complete, so the app never sees half of one
const PART = ".push-tmp";

function libraryDir() {
  if (process.env.CJK_DICT_LIBRARY)
    return resolve(process.env.CJK_DICT_LIBRARY);
  if (existsSync(".env")) {
    for (const line of readFileSync(".env", "utf8").split(/\r?\n/)) {
      const m = line.match(/^\s*CJK_DICT_LIBRARY\s*=\s*(.*?)\s*$/);
      if (m && m[1]) return resolve(m[1].replace(/^["']|["']$/g, ""));
    }
  }
  return resolve("library");
}

/** `adb shell <cmd>`: stdout, or null on failure */
function adbShell(cmd) {
  const r = spawnSync("adb", ["shell", cmd], { encoding: "utf8" });
  if (r.error) throw new Error(`adb: ${r.error.message}`);
  return r.status === 0 ? r.stdout.replace(/\r/g, "") : null;
}

/** run `script` with sh in the app's data folder */
const runAs = (script) => adbShell(`run-as ${PKG} sh -c '${script}'`);

/** Stream `file` (`size` bytes) into `dest` (relative to the app's data folder); resolves
 * to seconds taken */
async function pushFile(file, size, dest) {
  const t = Date.now();
  await new Promise((ok, fail) => {
    // one argument: adb joins them into a command line for the device's shell, which
    // would otherwise apply the redirect outside run-as
    const child = spawn(
      "adb",
      ["exec-in", `run-as ${PKG} sh -c 'cat > ${dest}${PART}'`],
      {
        stdio: ["pipe", "ignore", "pipe"],
      },
    );
    let err = "";
    child.stderr.on("data", (d) => (err += d));
    child.on("error", fail);
    child.on("close", (code) =>
      code === 0 ? ok() : fail(new Error(err.trim() || `exit ${code}`)),
    );
    createReadStream(file).on("error", fail).pipe(child.stdin);
  });
  // renamed only when whole; in a call of its own, as exec-in ends the command when its
  // input does
  const done = runAs(
    `[ "$(stat -c %s ${dest}${PART})" = ${size} ] && chmod 600 ${dest}${PART} && mv ${dest}${PART} ${dest} && echo ok`,
  );
  if (done?.trim() !== "ok") throw new Error("incomplete on the device");
  return (Date.now() - t) / 1000;
}

const mb = (n) => `${(n / 1e6).toFixed(1)} MB`;

async function main() {
  const lib = libraryDir();
  const manifestPath = join(lib, MANIFEST);
  if (!existsSync(manifestPath)) {
    console.log(
      `no ${manifestPath}: run \`pnpm admin build\` first. Nothing pushed.`,
    );
    return;
  }
  const m = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (!m.search_db_zst) {
    console.log(
      `${MANIFEST} has no search_db_zst (built by an older dict-admin). Nothing pushed.`,
    );
    return;
  }

  if (adbShell(`pm path ${PKG}`) === null) {
    console.log(
      `${PKG} isn't installed yet: nothing pushed; the app downloads the library.`,
    );
    return;
  }
  if (runAs("true") === null) {
    console.log(`run-as ${PKG} failed (a release build?): nothing pushed.`);
    return;
  }
  // the app must not plan or commit while files arrive; `tauri android run` starts it again
  adbShell(`am force-stop ${PKG}`);

  // what the app has: file sizes in dicts/ and dicts/incoming/, and its committed manifest
  const sizes = new Map();
  const listing =
    runAs(
      "mkdir -p dicts/incoming && cd dicts && rm -f incoming/*" +
        PART +
        ' && for f in * incoming/*; do [ -f "$f" ] && stat -c "%s %n" "$f"; done; true',
    ) ?? "";
  for (const line of listing.split("\n")) {
    const m = line.match(/^(\d+) (.+)$/);
    if (m) sizes.set(m[2], Number(m[1]));
  }
  let committed = null;
  const committedText = runAs(`cat dicts/${MANIFEST} 2>/dev/null`) || "";
  try {
    committed = JSON.parse(committedText || "null");
  } catch {
    // none, or unreadable: nothing is vouched for
  }
  const vouched = new Map(
    (committed?.dicts ?? []).map((d) => [d.file, d.sha256]),
  );
  const committedDb =
    committed?.search_db_zst?.unpacked_sha256 ?? committed?.search_db?.sha256;

  const files = [
    ...m.dicts.map((d) => ({ file: d.file, size: d.size, sha256: d.sha256 })),
    m.search_db_zst,
  ];
  console.log(`Library: ${lib}`);
  let pushed = 0;
  let failed = 0;
  // in incoming/ from an earlier push: the app commits them with the manifest
  let waiting = 0;
  for (const f of files) {
    const name = f.file;
    let why = null;
    if (name === m.search_db_zst.file) {
      if (
        sizes.has(SEARCH_DB) &&
        committedDb === m.search_db_zst.unpacked_sha256
      ) {
        why = `the app's ${SEARCH_DB} is current`;
      }
    } else if (sizes.get(name) === f.size && vouched.get(name) === f.sha256) {
      why = "in place";
    } else if (sizes.get(name) === f.size && !vouched.has(name)) {
      why = "in place, not yet committed";
    }
    if (!why && sizes.get(`incoming/${name}`) === f.size) {
      why = "already in incoming/";
      waiting++;
    }
    if (why) {
      console.log(`skip  ${name}: ${why}`);
      continue;
    }

    const local = join(lib, name);
    if (!existsSync(local) || statSync(local).size !== f.size) {
      console.log(
        `FAIL  ${name}: missing in the library or of another size than its manifest`,
      );
      failed++;
      continue;
    }
    try {
      const s = await pushFile(local, f.size, `dicts/incoming/${name}`);
      console.log(`push  ${name}: ${mb(f.size)} in ${s.toFixed(1)} s`);
      pushed++;
    } catch (e) {
      console.log(`FAIL  ${name}: ${e.message}`);
      failed++;
    }
  }

  // the manifest last, and only with every file there: offline, the app plans from it
  if (failed) {
    console.log(`skip  ${MANIFEST}: not every file was pushed`);
  } else if (
    !pushed &&
    !waiting &&
    committedText === readFileSync(manifestPath, "utf8")
  ) {
    console.log(`skip  ${MANIFEST}: the app has this library`);
  } else {
    try {
      const size = statSync(manifestPath).size;
      await pushFile(manifestPath, size, `dicts/incoming/${MANIFEST}`);
      console.log(`push  ${MANIFEST}`);
    } catch (e) {
      console.log(`FAIL  ${MANIFEST}: ${e.message}`);
      failed++;
    }
  }
  console.log(
    `${pushed} pushed, ${failed} failed. The app checks them when it syncs. ` +
      "With a remote it reaches, it plans from the remote's manifest: run " +
      "`pnpm admin upload` first. Without one, from the pushed manifest.",
  );
}

main().catch((e) => {
  // never stop `tauri android run`: the app downloads what wasn't pushed
  console.log(`push-library: ${e.message}. Nothing more pushed.`);
});
