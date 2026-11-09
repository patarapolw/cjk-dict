/**
 * Read-only dictionary API over D1 (search index + entries) and R2 (media packs).
 * Dictionaries are only added/removed by `dict-admin`.
 *
 * GET  /dicts                         -> IDict[]
 * POST /search                        -> { items: ISearchItem[], next: number | null }
 * GET  /dicts/:name/assets/<path>     -> media file (images ...) referenced by structured content
 */

import { expandPinyin } from "./pinyin";

export interface Env {
  DB: D1Database;
  BUCKET: R2Bucket;
  CORS_ORIGIN?: string;
}

type ShortLang = "ja" | "zh" | "ko";
const SHORT_LANGS: ShortLang[] = ["ja", "zh", "ko"];

export interface SearchRequest {
  /** "fts" = token search; "exact" = term equality; anything else = prefix search */
  type?: string | null;
  /** e.g. "ja-JP"; empty = all languages */
  lang?: string;
  term?: string;
  /** reading; pinyin is expanded to every tone combination when lang is zh-CN or empty */
  reading?: string;
  /** fts only: client-side tokenized `term`, space-separated, per language */
  tokens?: Partial<Record<ShortLang, string>>;
  limit?: number;
  offset?: number;
}

const MAX_LIMIT = 50;
/** 5^4: up to four toneless syllables are expanded; beyond that, match the raw reading */
const MAX_READINGS = 625;

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const cors = corsHeaders(req, env);
    if (req.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: cors });
    }

    try {
      const res = await route(req, env);
      for (const [k, v] of Object.entries(cors)) res.headers.set(k, v);
      return res;
    } catch (e) {
      if (e instanceof HttpError) {
        return json({ error: e.message }, e.status, cors);
      }
      console.error(e);
      return json({ error: "internal error" }, 500, cors);
    }
  },
} satisfies ExportedHandler<Env>;

class HttpError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

function corsHeaders(req: Request, env: Env): Record<string, string> {
  const allowed = (env.CORS_ORIGIN || "*").split(",").map((s) => s.trim());
  const origin = req.headers.get("Origin") || "";
  const allow = allowed.includes("*")
    ? "*"
    : allowed.includes(origin)
      ? origin
      : "";
  if (!allow) return {};
  return {
    "Access-Control-Allow-Origin": allow,
    "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
    // structured-content <img> are loaded cross-origin
    "Cross-Origin-Resource-Policy": "cross-origin",
    "Access-Control-Allow-Headers": "Content-Type",
    "Access-Control-Max-Age": "86400",
  };
}

function json(data: unknown, status = 200, headers: Record<string, string> = {}) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "Content-Type": "application/json; charset=utf-8", ...headers },
  });
}

async function route(req: Request, env: Env): Promise<Response> {
  const url = new URL(req.url);
  const parts = url.pathname.split("/").filter(Boolean).map(decodeURIComponent);

  if (req.method === "GET" && parts.length === 1 && parts[0] === "dicts") {
    return json(await listDicts(env));
  }

  if (req.method === "POST" && parts.length === 1 && parts[0] === "search") {
    let body: SearchRequest;
    try {
      body = await req.json();
    } catch {
      throw new HttpError(400, "invalid JSON body");
    }
    return json(await search(env, body));
  }

  if (req.method === "GET" && parts[0] === "dicts" && parts.length >= 3) {
    const name = parts[1];
    if (parts[2] === "assets" && parts.length > 3) {
      return await getAsset(env, name, parts.slice(3).join("/"), req);
    }
  }

  throw new HttpError(404, "not found");
}

export interface IDict {
  id: number;
  name: string;
  title: string;
  revision: string | null;
  L1: string | null;
  L2: string | null;
  term_count: number;
  source: string | null;
  created_at: number;
}

async function listDicts(env: Env): Promise<IDict[]> {
  const { results } = await env.DB.prepare(
    `SELECT id, name, title, revision, L1, L2, term_count, source, created_at
     FROM dict WHERE status = 'ready' ORDER BY id`,
  ).all<IDict>();
  return results;
}

export interface ISearchTerm {
  id: number;
  term: string;
  term_ja: string;
  term_zh: string;
  term_ko: string;
  reading: string;
  def_tags: string | null;
  rules: string | null;
  score: number | null;
  tags: string | null;
}

export interface ITerm {
  dict_id: number;
  /** dictionary name */
  dict_path: string;
  term_id: number;
  term: string;
  reading: string;
  def_tags: string;
  rules: string;
  score: number | null;
  glossary: string;
  sequence: number | null;
  tags: string;
}

function ftsPhrase(s: string) {
  return `"${s.replace(/"/g, " ")}"`;
}

function readingMatch(readings: string[]) {
  const qs = readings.map(ftsPhrase).join(" OR ");
  return "reading:" + (readings.length > 1 ? `(${qs})` : qs);
}

function expandReading(reading: string, lang: ShortLang | ""): string[] {
  reading = reading.trim();
  if (!reading) return [];
  if (!lang || lang === "zh") {
    const expanded = expandPinyin(reading);
    if (expanded.length && expanded.length <= MAX_READINGS) return expanded;
  }
  return [reading];
}

function shortLang(lang?: string): ShortLang | "" {
  const l = (lang || "").split("-")[0] as ShortLang;
  return SHORT_LANGS.includes(l) ? l : "";
}

async function search(env: Env, body: SearchRequest) {
  const limit = Math.min(Math.max(1, Math.floor(body.limit ?? 10)), MAX_LIMIT);
  const offset = Math.max(0, Math.floor(body.offset ?? 0));
  const lang = shortLang(body.lang);
  const readings = expandReading(
    typeof body.reading === "string" ? body.reading : "",
    lang,
  );

  let items: ISearchTerm[];

  if (body.type === "fts") {
    const kvs = (lang ? [lang] : SHORT_LANGS)
      .map((l) => [l, (body.tokens?.[l] || "").trim()] as const)
      .filter(([, v]) => v);

    let qMatch = kvs.map(([k, v]) => `term_${k}:${ftsPhrase(v)}`).join(" OR ");
    if (readings.length) {
      // Reading expansion can get large. Put term match first.
      qMatch = qMatch
        ? `(${qMatch}) AND ${readingMatch(readings)}`
        : readingMatch(readings);
    }

    items = qMatch
      ? (
          await env.DB.prepare(
            `SELECT s.* FROM term_search_fts f
             JOIN term_search s ON s.id = f.rowid
             WHERE term_search_fts MATCH ?1
             ORDER BY f.rank
             LIMIT ?2 OFFSET ?3`,
          )
            .bind(qMatch, limit + 1, offset)
            .all<ISearchTerm>()
        ).results
      : [];
  } else {
    const isExact = body.type === "exact";
    let qTerm = (body.term || "").normalize("NFKC").toLocaleUpperCase();
    if (!isExact && qTerm && !qTerm.endsWith(" ")) qTerm += "*";

    const where: string[] = [];
    const params: unknown[] = [];
    if (lang) where.push(`term_${lang} != ''`);
    if (qTerm) {
      params.push(qTerm);
      where.push(`term ${isExact ? "=" : "GLOB"} ?${params.length}`);
    }
    if (readings.length) {
      params.push(readingMatch(readings));
      where.push(
        `id IN (SELECT rowid FROM term_search_fts WHERE term_search_fts MATCH ?${params.length})`,
      );
    }

    items =
      qTerm || readings.length
        ? (
            await env.DB.prepare(
              `SELECT * FROM term_search
               WHERE ${where.join(" AND ")}
               ORDER BY score DESC
               LIMIT ${limit + 1} OFFSET ${offset}`,
            )
              .bind(...params)
              .all<ISearchTerm>()
          ).results
        : [];
  }

  let next: number | null = null;
  if (items.length > limit) {
    items.pop();
    next = offset + limit;
  }

  return { items: await expandItems(env, items), next };
}

async function expandItems(env: Env, items: ISearchTerm[]) {
  if (!items.length) return [];

  const ids = items.map((it) => it.id);
  const { results } = await env.DB.prepare(
    `SELECT t.search_id, t.dict_id, d.name AS dict_path, t.id AS term_id,
            t.term, t.reading, t.def_tags, t.rules, t.score, t.glossary, t."sequence", t.tags
     FROM term t JOIN dict d ON d.id = t.dict_id
     WHERE t.search_id IN (${ids.map((_, i) => `?${i + 1}`).join(",")})
       AND d.status = 'ready'
     ORDER BY t.dict_id, t.id`,
  )
    .bind(...ids)
    .all<ITerm & { search_id: number }>();

  const bySearchId = new Map<number, ITerm[]>();
  for (const { search_id, ...t } of results) {
    const arr = bySearchId.get(search_id) || [];
    arr.push(t);
    bySearchId.set(search_id, arr);
  }

  return items.map((it) => ({ ...it, entries: bySearchId.get(it.id) || [] }));
}

async function getAsset(env: Env, name: string, path: string, req: Request) {
  const a = await env.DB.prepare(
    `SELECT d.id AS dict_id, d.assets_key, a."offset", a.size
     FROM asset a JOIN dict d ON d.id = a.dict_id
     WHERE d.name = ?1 AND d.status = 'ready' AND a."path" = ?2`,
  )
    .bind(name, path)
    .first<{ dict_id: number; assets_key: string | null; offset: number; size: number }>();
  if (!a || !a.assets_key) throw new HttpError(404, "asset not found");

  const etag = `"${a.dict_id}-${a.offset}-${a.size}"`;
  if (req.headers.get("If-None-Match") === etag) {
    return new Response(null, { status: 304, headers: { ETag: etag } });
  }

  const obj = await env.BUCKET.get(a.assets_key, {
    range: { offset: a.offset, length: a.size },
  });
  if (!obj) throw new HttpError(404, "media pack not found");

  const ext = path.split(".").pop()?.toLowerCase() || "";
  return new Response(obj.body, {
    headers: {
      "Content-Type": MIME[ext] || "application/octet-stream",
      "Content-Length": String(a.size),
      "Cache-Control": "public, max-age=31536000, immutable",
      ETag: etag,
    },
  });
}

const MIME: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  avif: "image/avif",
  svg: "image/svg+xml",
  bmp: "image/bmp",
  ico: "image/x-icon",
  css: "text/css",
  txt: "text/plain; charset=utf-8",
  json: "application/json",
  mp3: "audio/mpeg",
  ogg: "audio/ogg",
  wav: "audio/wav",
  opus: "audio/opus",
};
