#!/usr/bin/env node
// Push the dict-admin library to the app, so the app finds its files already downloaded
// rather than downloading them from R2.
//
//   node scripts/push-library.mjs            to the Android app, over adb (`pnpm android:push`;
//                                            `pnpm android` runs it before `tauri android run`)
//   node scripts/push-library.mjs --desktop  to the desktop app on this computer, by copying
//                                            into its app data folder (`pnpm desktop:push`)
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
// - Android: sends each file a chunk at a time into a .push-tmp, and resumes one left by an
//   interrupted push.
// - Android: needs the app installed (a debug build, for `run-as`): on a first install it
//   pushes nothing, and the app downloads the library. adb picks the device as usual: set
//   ANDROID_SERIAL when several are connected.
// - Desktop: any build, installed or not; close the app first. The folder is
//   `cc.polv.cjk-dict/dicts` in %APPDATA% (Windows), ~/Library/Application Support (macOS)
//   or $XDG_DATA_HOME, else ~/.local/share (Linux).

import { spawn, spawnSync } from "node:child_process";
import {
  copyFileSync,
  createReadStream,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
} from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

const PKG = "cc.polv.cjk_dict";
// the desktop app's identifier (tauri.conf.json), its app data folder's name
const IDENTIFIER = "cc.polv.cjk-dict";
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

// bytes per `adb exec-in`: a long stream may stop early, or drop the device, and exec-in
// exits 0 all the same; a chunk at a time, only the chunk is sent again
const CHUNK = 32 * 1024 * 1024;
// chunks in a row that may add nothing before the file is given up
const STALLS = 3;

/** Append bytes `start` to `end` (exclusive) of `file` to `tmp` on the device */
function appendChunk(file, start, end, tmp) {
  return new Promise((ok, fail) => {
    // one argument: adb joins them into a command line for the device's shell, which
    // would otherwise apply the redirect outside run-as
    const child = spawn(
      "adb",
      ["exec-in", `run-as ${PKG} sh -c 'cat >> ${tmp}'`],
      {
        stdio: ["pipe", "ignore", "pipe"],
      },
    );
    let err = "";
    child.stderr.on("data", (d) => (err += d));
    child.on("error", fail);
    // EPIPE when adb ends before its input does: the size check after says how far it got
    child.stdin.on("error", () => {});
    child.on("close", (code) =>
      code === 0 ? ok() : fail(new Error(err.trim() || `adb exit ${code}`)),
    );
    createReadStream(file, { start, end: end - 1 })
      .on("error", fail)
      .pipe(child.stdin);
  });
}

/** The size of `path` on the device, or -1 when it's missing (or adb can't tell) */
function deviceSize(path) {
  const out = runAs(`stat -c %s ${path} 2>/dev/null || echo -1`);
  const n = Number(out?.trim());
  return Number.isInteger(n) ? n : -1;
}

/** Stream `file` (`size` bytes) into `dest` (relative to the app's data folder), resuming
 * a `.push-tmp` of `have` bytes left by an earlier push; resolves to seconds taken */
async function pushFile(file, size, dest, have = 0) {
  const t = Date.now();
  const tmp = dest + PART;
  let offset = have;
  if (offset > size) {
    runAs(`rm -f ${tmp}`);
    offset = 0;
  }
  let stalls = 0;
  while (offset < size) {
    const end = Math.min(offset + CHUNK, size);
    try {
      await appendChunk(file, offset, end, tmp);
    } catch {
      // adb itself failed: the size below says whether the device is still there
    }
    const now = deviceSize(tmp);
    if (now === -1) {
      // the device dropped off: the next push resumes the .push-tmp
      throw new Error(`the device stopped answering at ${mb(offset)}`);
    }
    if (now < offset || now > end) {
      // something else wrote to it: start over
      runAs(`rm -f ${tmp}`);
      throw new Error(`${tmp} changed on the device while it was pushed`);
    }
    if (now === offset) {
      if (++stalls >= STALLS)
        throw new Error(`no progress at ${mb(offset)}, ${STALLS} times`);
    } else {
      stalls = 0;
    }
    offset = now;
  }
  // renamed only when whole; in a call of its own, as exec-in ends the command when its
  // input does
  const done = runAs(
    `[ "$(stat -c %s ${tmp})" = ${size} ] && chmod 600 ${tmp} && mv ${tmp} ${dest} && echo ok`,
  );
  if (done?.trim() !== "ok") throw new Error("incomplete on the device");
  return (Date.now() - t) / 1000;
}

/** The Android app, over adb; null (having said why) when it can't take the library */
function androidTarget() {
  if (adbShell(`pm path ${PKG}`) === null) {
    console.log(
      `${PKG} isn't installed yet: nothing pushed; the app downloads the library.`,
    );
    return null;
  }
  if (runAs("true") === null) {
    console.log(`run-as ${PKG} failed (a release build?): nothing pushed.`);
    return null;
  }
  // the app must not plan or commit while files arrive; `tauri android run` starts it again
  adbShell(`am force-stop ${PKG}`);
  return {
    sizes() {
      const sizes = new Map();
      const listing =
        runAs(
          // .push-tmp files included: pushes resume them
          "mkdir -p dicts/incoming && cd dicts" +
            ' && for f in * incoming/*; do [ -f "$f" ] && stat -c "%s %n" "$f"; done; true',
        ) ?? "";
      for (const line of listing.split("\n")) {
        const m = line.match(/^(\d+) (.+)$/);
        if (m) sizes.set(m[2], Number(m[1]));
      }
      return sizes;
    },
    committedManifest: () => runAs(`cat dicts/${MANIFEST} 2>/dev/null`) || "",
    push: (file, size, name, have) =>
      pushFile(file, size, `dicts/incoming/${name}`, have),
  };
}

/** The desktop app's `dicts/` folder on this computer */
function desktopDictsDir() {
  const home = homedir();
  const data =
    process.platform === "win32"
      ? process.env.APPDATA || join(home, "AppData", "Roaming")
      : process.platform === "darwin"
        ? join(home, "Library", "Application Support")
        : process.env.XDG_DATA_HOME || join(home, ".local", "share");
  return join(data, IDENTIFIER, "dicts");
}

/** The desktop app on this computer, by copying into its app data folder */
function desktopTarget() {
  const dir = desktopDictsDir();
  const incoming = join(dir, "incoming");
  mkdirSync(incoming, { recursive: true });
  console.log(`App data: ${dir} (close the app first)`);
  return {
    sizes() {
      const sizes = new Map();
      for (const [sub, prefix] of [
        [dir, ""],
        [incoming, "incoming/"],
      ]) {
        for (const e of readdirSync(sub, { withFileTypes: true })) {
          if (!e.isFile()) continue;
          if (e.name.endsWith(PART)) {
            rmSync(join(sub, e.name), { force: true });
            continue;
          }
          sizes.set(prefix + e.name, statSync(join(sub, e.name)).size);
        }
      }
      return sizes;
    },
    committedManifest() {
      try {
        return readFileSync(join(dir, MANIFEST), "utf8");
      } catch {
        return "";
      }
    },
    async push(file, size, name) {
      const t = Date.now();
      const dest = join(incoming, name);
      copyFileSync(file, dest + PART);
      if (statSync(dest + PART).size !== size)
        throw new Error("incomplete copy");
      renameSync(dest + PART, dest);
      return (Date.now() - t) / 1000;
    },
  };
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

  const target = process.argv.includes("--desktop")
    ? desktopTarget()
    : androidTarget();
  if (!target) return;

  // what the app has: file sizes in dicts/ and dicts/incoming/, and its committed manifest
  const sizes = target.sizes();
  const committedText = target.committedManifest();
  let committed = null;
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
      const have = sizes.get(`incoming/${name}${PART}`) ?? 0;
      const s = await target.push(local, f.size, name, have);
      const resumed = have ? `, resumed at ${mb(have)}` : "";
      console.log(
        `push  ${name}: ${mb(f.size)} in ${s.toFixed(1)} s${resumed}`,
      );
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
      await target.push(manifestPath, size, MANIFEST);
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
