#!/usr/bin/env node
// Print what dictionaries say about themselves in their index.json: title, revision,
// sourceLanguage and targetLanguage, next to dict-admin's overrides. The output is a
// Markdown table, to paste into a pull request or a conversation.
//
//   node scripts/dict-index.mjs                     # the default dictionaries (dict-admin)
//   node scripts/dict-index.mjs library             # every zip in a directory
//   node scripts/dict-index.mjs a.zip https://example.com/b.zip
//
// With no arguments it runs `pnpm admin defaults --list` (cargo, and GitHub's API: set
// GITHUB_TOKEN if rate limited). URLs are read with HTTP range requests, so only the zip's
// directory and index.json are downloaded, not the whole dictionary.

import { execFileSync } from "node:child_process";
import { openSync, readSync, fstatSync, closeSync, statSync } from "node:fs";
import { readdirSync } from "node:fs";
import { basename, join } from "node:path";
import { inflateRawSync } from "node:zlib";

/** random access to a local file or a URL: `size` and `read(offset, length)` */
async function source(location) {
  if (/^https?:\/\//.test(location)) {
    // follow redirects (GitHub release assets) to where ranges are served
    const head = await fetch(location, { headers: { Range: "bytes=0-0" } });
    if (!head.ok) throw new Error(`${location}: HTTP ${head.status}`);
    const url = head.url;
    const total = head.headers.get("content-range")?.split("/")[1];
    await head.body?.cancel();
    if (head.status !== 206 || !total || total === "*") {
      // no range support: the whole file
      const buf = Buffer.from(await (await fetch(url)).arrayBuffer());
      return { size: buf.length, read: async (o, n) => buf.subarray(o, o + n) };
    }
    return {
      size: Number(total),
      read: async (offset, length) => {
        const res = await fetch(url, {
          headers: { Range: `bytes=${offset}-${offset + length - 1}` },
        });
        if (res.status !== 206) throw new Error(`${url}: HTTP ${res.status}`);
        return Buffer.from(await res.arrayBuffer());
      },
    };
  }
  const fd = openSync(location, "r");
  return {
    size: fstatSync(fd).size,
    read: async (offset, length) => {
      const buf = Buffer.alloc(length);
      const n = readSync(fd, buf, 0, length, offset);
      return buf.subarray(0, n);
    },
    close: () => closeSync(fd),
  };
}

/** one file of a zip, by name, reading only what's needed */
async function readZipEntry(src, name) {
  // end of central directory: in the last 22 bytes + up to 64 KiB of comment
  const tailLen = Math.min(src.size, 22 + 0xffff);
  const tail = await src.read(src.size - tailLen, tailLen);
  const eocd = tail.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0) throw new Error("not a zip");
  let cdSize = tail.readUInt32LE(eocd + 12);
  let cdOffset = tail.readUInt32LE(eocd + 16);
  if (cdOffset === 0xffffffff || cdSize === 0xffffffff) {
    // zip64: its end-of-central-directory record, located by the locator before EOCD
    const loc = eocd - 20;
    if (loc < 0 || tail.readUInt32LE(loc) !== 0x07064b50) {
      throw new Error("zip64 locator not found");
    }
    const recOffset = Number(tail.readBigUInt64LE(loc + 8));
    const rec = await src.read(recOffset, 56);
    cdSize = Number(rec.readBigUInt64LE(40));
    cdOffset = Number(rec.readBigUInt64LE(48));
  }
  const cd = await src.read(cdOffset, cdSize);
  for (let p = 0; p + 46 <= cd.length && cd.readUInt32LE(p) === 0x02014b50; ) {
    const method = cd.readUInt16LE(p + 10);
    let compSize = cd.readUInt32LE(p + 20);
    const nameLen = cd.readUInt16LE(p + 28);
    const extraLen = cd.readUInt16LE(p + 30);
    const commentLen = cd.readUInt16LE(p + 32);
    let localOffset = cd.readUInt32LE(p + 42);
    const entryName = cd.toString("utf8", p + 46, p + 46 + nameLen);
    if (entryName === name) {
      // zip64 extra field: 64-bit sizes and offset for the fields set to 0xffffffff
      let e = p + 46 + nameLen;
      const end = e + extraLen;
      while (e + 4 <= end) {
        const id = cd.readUInt16LE(e);
        const len = cd.readUInt16LE(e + 2);
        if (id === 1) {
          let f = e + 4;
          if (cd.readUInt32LE(p + 24) === 0xffffffff) f += 8; // uncompressed size
          if (compSize === 0xffffffff) {
            compSize = Number(cd.readBigUInt64LE(f));
            f += 8;
          }
          if (localOffset === 0xffffffff)
            localOffset = Number(cd.readBigUInt64LE(f));
        }
        e += 4 + len;
      }
      const local = await src.read(localOffset, 30);
      const dataOffset =
        localOffset + 30 + local.readUInt16LE(26) + local.readUInt16LE(28);
      const data = await src.read(dataOffset, compSize);
      if (method === 0) return data;
      if (method === 8) return inflateRawSync(data);
      throw new Error(`${name}: compression method ${method}`);
    }
    p += 46 + nameLen + extraLen + commentLen;
  }
  throw new Error(`no ${name}`);
}

/** [name, location, dict-admin's L1, dict-admin's L2] */
function defaults() {
  let out;
  try {
    out = execFileSync(
      "cargo",
      [
        "run",
        "--release",
        "-q",
        "-p",
        "dict-admin",
        "--",
        "defaults",
        "--list",
      ],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
  } catch (e) {
    const why = String(e.stderr ?? e.message).split("\n")[0];
    console.error(`pnpm admin defaults --list failed: ${why}`);
    console.error("Pass zips, a directory of them or URLs instead.");
    process.exit(1);
  }
  return out
    .trim()
    .split("\n")
    .map((line) => {
      const [name, lang, targetLang, url] = line.split("\t");
      return [name, url, lang, targetLang];
    });
}

function fromArgs(args) {
  return args.flatMap((a) => {
    if (!/^https?:\/\//.test(a) && statSync(a).isDirectory()) {
      return readdirSync(a)
        .filter((f) => f.endsWith(".zip"))
        .sort()
        .map((f) => [basename(f, ".zip"), join(a, f), "", ""]);
    }
    return [[basename(a.split("?")[0], ".zip"), a, "", ""]];
  });
}

const args = process.argv.slice(2);
const dicts = args.length ? fromArgs(args) : defaults();
/** an error on one line, with its cause (fetch's "fetch failed" says little alone) */
const errorText = (e) =>
  [e?.message ?? e, e?.cause?.message ?? e?.cause?.code]
    .filter(Boolean)
    .join(": ")
    .replace(/\s+/g, " ")
    .replace(/\|/g, "\\|");
const cell = (v) =>
  v === undefined || v === null || v === "" ? "-" : String(v);

console.log(
  "| name | sourceLanguage | targetLanguage | dict-admin L1 | dict-admin L2 | title | revision |",
);
console.log("| --- | --- | --- | --- | --- | --- | --- |");
for (const [name, location, l1, l2] of dicts) {
  let src;
  try {
    src = await source(location);
    const index = JSON.parse(
      (await readZipEntry(src, "index.json")).toString("utf8"),
    );
    console.log(
      `| ${name} | ${cell(index.sourceLanguage)} | ${cell(index.targetLanguage)} | ${cell(l1)} | ${cell(l2)} | ${cell(index.title)} | ${cell(index.revision)} |`,
    );
  } catch (e) {
    console.log(
      `| ${name} | error: ${errorText(e)} | | ${cell(l1)} | ${cell(l2)} | | |`,
    );
  } finally {
    src?.close?.();
  }
}
