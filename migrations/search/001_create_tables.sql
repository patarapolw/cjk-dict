CREATE TABLE "index" (
  "path"        TEXT NOT NULL PRIMARY KEY,  -- actual YomitanReader root_dir

  L1            TEXT,                       -- data[$.sourceLanguage] or enforced by user
  L2            TEXT,                       -- data[$.targetLanguage] or enforced by user
  "data"        TEXT_JSON NOT NULL
);

CREATE TABLE term (
  term        TEXT NOT NULL,
  term_ja     TEXT NOT NULL,
  term_zh     TEXT NOT NULL,
  term_ko     TEXT NOT NULL,

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

CREATE VIEW term_tags AS
SELECT
  term, reading, rowid,
  def_tags, rules, tags
FROM term;

CREATE VIRTUAL TABLE term_tags_fts USING fts5 (
  term        UNINDEXED,
  reading     UNINDEXED,
  def_tags,
  rules,
  tags,
  content='term_tags'
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_tags_afer_insert AFTER INSERT ON term
BEGIN
  INSERT INTO term_tags_fts(
    def_tags,
    rules,
    tags,
  rowid) VALUES (
    new.def_tags,
    new.rules,
    new.tags,
  new.rowid);
END;
