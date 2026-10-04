CREATE TABLE "index" (
  "path"        TEXT NOT NULL PRIMARY KEY,  -- actual YomitanReader root_dir

  L1            TEXT,                       -- data[$.sourceLanguage] or enforced by user
  L2            TEXT,                       -- data[$.targetLanguage] or enforced by user
  "data"        TEXT_JSON NOT NULL
);

CREATE TABLE term (
  term        TEXT NOT NULL,
  term_ja     TEXT,
  term_zh     TEXT,
  term_ko     TEXT,

  reading     TEXT NOT NULL,  -- DEFAULT ''

  def_tags    TEXT, -- space-separated
  rules       TEXT, -- space-separated
  score       INTEGER,
  -- glossary -- don't include in search table at all
  "sequence"  INTEGER,
  tags        TEXT, -- space-separated

  index_rowid     INTEGER NOT NULL REFERENCES "index" (rowid),
  term_rowid      INTEGER NOT NULL
);

CREATE INDEX idx_term_term ON term (term);
CREATE INDEX idx_term_reading ON term (reading);
CREATE INDEX idx_term_score ON term (score);
CREATE INDEX idx_term_sequence ON term ("sequence");

CREATE VIRTUAL TABLE term_fts USING fts5 (
  term        UNINDEXED,
  term_ja,
  term_zh,
  term_ko,
  reading     UNINDEXED,
  def_tags,
  rules,
  score       UNINDEXED,  -- consider sorting non-TEXT outside
  "sequence"  UNINDEXED,
  tags,
  index_rowid UNINDEXED,
  content='term'
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_afer_insert AFTER INSERT ON term
BEGIN
  INSERT INTO term_fts(
    term_ja,
    term_zh,
    term_ko,
    def_tags,
    rules,
    tags,
  rowid) VALUES (
    new.term_ja,
    new.term_zh,
    new.term_ko,
    new.def_tags,
    new.rules,
    new.tags,
  new.rowid);
END;
