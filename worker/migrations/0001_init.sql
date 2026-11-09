-- One row per imported Yomitan dictionary. Zips are not stored: entries live in D1, and
-- media files (if any) are packed, uncompressed, into a single R2 object at assets_key.
-- Rows are only written by dict-admin; the Worker only reads `status = 'ready'` dicts.
CREATE TABLE dict (
  id          INTEGER PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,   -- key used by the client (was the local folder path)
  title       TEXT NOT NULL,
  revision    TEXT,
  L1          TEXT,                   -- source language, e.g. ja-JP (enforced by admin or index.json)
  L2          TEXT,                   -- target language
  index_json  TEXT,                   -- raw index.json
  "source"    TEXT,                   -- URL or file name the dictionary was imported from
  assets_key  TEXT,                   -- R2 key of the media pack; NULL if the dict has no media
  assets_size INTEGER,
  term_count  INTEGER NOT NULL DEFAULT 0,
  status      TEXT NOT NULL DEFAULT 'importing' CHECK (status IN ('importing', 'ready', 'deleting')),
  created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Yomitan banks, with dict_id. (No FK constraints: dict-admin deletes in batches.)
CREATE TABLE term (
  id          INTEGER PRIMARY KEY,
  dict_id     INTEGER NOT NULL,
  search_id   INTEGER NOT NULL,       -- term_search.id
  term        TEXT NOT NULL,
  reading     TEXT NOT NULL,
  def_tags    TEXT,
  rules       TEXT,
  score       INTEGER,
  glossary    TEXT,                   -- JSON array
  "sequence"  INTEGER,
  tags        TEXT
);
CREATE INDEX idx_term_search_id ON term (search_id);
CREATE INDEX idx_term_dict_id ON term (dict_id);

CREATE TABLE term_meta (
  dict_id     INTEGER NOT NULL,
  term        TEXT NOT NULL,
  "type"      TEXT NOT NULL,
  "data"      TEXT NOT NULL
);
CREATE INDEX idx_term_meta_term ON term_meta (term);
CREATE INDEX idx_term_meta_dict_id ON term_meta (dict_id);

CREATE TABLE kanji (
  dict_id     INTEGER NOT NULL,
  kanji       TEXT NOT NULL,
  onyomi      TEXT,
  kunyomi     TEXT,
  tags        TEXT,
  meanings    TEXT NOT NULL DEFAULT '[]',
  "stats"     TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_kanji_kanji ON kanji (kanji);
CREATE INDEX idx_kanji_dict_id ON kanji (dict_id);

CREATE TABLE kanji_meta (
  dict_id     INTEGER NOT NULL,
  kanji       TEXT NOT NULL,
  "type"      TEXT NOT NULL,
  "data"      INTEGER NOT NULL
);
CREATE INDEX idx_kanji_meta_kanji ON kanji_meta (kanji);
CREATE INDEX idx_kanji_meta_dict_id ON kanji_meta (dict_id);

CREATE TABLE tag (
  dict_id     INTEGER NOT NULL,
  "name"      TEXT NOT NULL,
  category    TEXT,
  sort_order  INTEGER,
  notes       TEXT,
  score       INTEGER
);
CREATE INDEX idx_tag_dict_id_name ON tag (dict_id, "name");

-- Media files (images ...) referenced by structured content `path`:
-- byte range inside the dict's R2 media pack.
CREATE TABLE asset (
  dict_id     INTEGER NOT NULL,
  "path"      TEXT NOT NULL,
  "offset"    INTEGER NOT NULL,
  size        INTEGER NOT NULL,
  UNIQUE (dict_id, "path")
);

-- Shared search index across dictionaries (was search.db).
CREATE TABLE term_search (
  id          INTEGER PRIMARY KEY,
  term        TEXT NOT NULL,
  term_ja     TEXT NOT NULL DEFAULT '', -- space-separated tokens
  term_zh     TEXT NOT NULL DEFAULT '',
  term_ko     TEXT NOT NULL DEFAULT '',
  reading     TEXT NOT NULL DEFAULT '' CHECK (reading = '' OR reading LIKE ' %_ '),
  def_tags    TEXT,
  rules       TEXT,
  score       INTEGER,                  -- max score
  tags        TEXT,
  "data"      TEXT
);

CREATE UNIQUE INDEX idx_u_term_search ON term_search (
  term, reading,
  (term_ja != ''), (term_zh != ''), (term_ko != '')
);
CREATE INDEX idx_term_search_score ON term_search (score);

CREATE VIRTUAL TABLE term_search_fts USING fts5 (
  "term"  UNINDEXED,
  "term_ja",
  "term_zh",
  "term_ko",
  "reading",
  "def_tags",
  "rules",
  "score"  UNINDEXED,
  "tags",
  "data"  UNINDEXED,
  content='term_search',
  content_rowid='id',
  tokenize="unicode61 tokenchars '-'"
);

CREATE TRIGGER t_term_search_after_insert AFTER INSERT ON term_search
BEGIN
  INSERT INTO term_search_fts (term_ja, term_zh, term_ko, reading, def_tags, rules, tags, rowid)
  VALUES (new.term_ja, new.term_zh, new.term_ko, new.reading, new.def_tags, new.rules, new.tags, new.rowid);
END;

CREATE TRIGGER t_term_search_after_delete AFTER DELETE ON term_search
BEGIN
  INSERT INTO term_search_fts (term_search_fts, rowid, term_ja, term_zh, term_ko, reading, def_tags, rules, tags)
  VALUES ('delete', old.rowid, old.term_ja, old.term_zh, old.term_ko, old.reading, old.def_tags, old.rules, old.tags);
END;

CREATE TRIGGER t_term_search_after_update AFTER UPDATE
OF term_ja, term_zh, term_ko, reading, def_tags, rules, tags ON term_search
BEGIN
  INSERT INTO term_search_fts (term_search_fts, rowid, term_ja, term_zh, term_ko, reading, def_tags, rules, tags)
  VALUES ('delete', old.rowid, old.term_ja, old.term_zh, old.term_ko, old.reading, old.def_tags, old.rules, old.tags);
  INSERT INTO term_search_fts (term_ja, term_zh, term_ko, reading, def_tags, rules, tags, rowid)
  VALUES (new.term_ja, new.term_zh, new.term_ko, new.reading, new.def_tags, new.rules, new.tags, new.rowid);
END;
