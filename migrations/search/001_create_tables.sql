CREATE TABLE "index" (
  "path"        TEXT NOT NULL PRIMARY KEY,  -- actual YomitanReader root_dir

  L1            TEXT NOT NULL, --data[$.sourceLanguage] ensure not missing
  L2            TEXT NOT NULL, --data[$.targetLanguage] ensure not missing
  "data"        TEXT_JSON NOT NULL
);

CREATE TABLE term (
  term        TEXT NOT NULL,
  term_ja     TEXT,
  term_zh     TEXT,
  term_ko     TEXT,

  reading     TEXT NOT NULL,  -- DEFAULT ''
  reading_ja  TEXT,
  reading_zh  TEXT,

  def_tags    TEXT, -- space-separated
  rules       TEXT, -- space-separated
  score       INTEGER,
  -- glossary -- don't include in search table at all
  "sequence"  INTEGER,
  tags        TEXT, -- space-separated

  L1          TEXT NOT NULL, -- index.L1
  L2          TEXT NOT NULL, -- index.L2
  index_rowid     INTEGER NOT NULL REFERENCES "index" (rowid),
  term_rowid      INTEGER NOT NULL
);

CREATE INDEX idx_term_term ON term (term);
CREATE INDEX idx_term_reading ON term (reading);
CREATE INDEX idx_term_score ON term (score);
CREATE INDEX idx_term_sequence ON term ("sequence");
CREATE INDEX idx_term_L1 ON term (L1);
CREATE INDEX idx_term_L2 ON term (L2);

CREATE VIRTUAL TABLE term_ja_fts USING fts5 (
  term        UNINDEXED,
  term_ja,
  reading     UNINDEXED,
  reading_ja,
  def_tags,
  rules,
  score       UNINDEXED,  -- consider sorting non-TEXT outside
  "sequence"  UNINDEXED,
  tags,
  L1          UNINDEXED,  -- ja-JP
  L2          UNINDEXED,
  index_rowid UNINDEXED,
  content='term'
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_ja_afer_insert AFTER INSERT ON term
WHEN new.L1 LIKE 'ja%'
BEGIN
  INSERT INTO term_ja_fts(
    term_ja,
    reading_ja,
    def_tags,
    rules,
    tags,
  rowid) VALUES (
    new.term_ja,
    new.reading_ja,
    new.def_tags,
    new.rules,
    new.tags,
  new.rowid);
END;
