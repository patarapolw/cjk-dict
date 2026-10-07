CREATE TABLE "index" (
  "path"        TEXT NOT NULL PRIMARY KEY,  -- actual YomitanReader root_dir

  L1            TEXT,                       -- data[$.sourceLanguage] or enforced by user
  L2            TEXT,                       -- data[$.targetLanguage] or enforced by user
  "data"        TEXT_JSON NOT NULL
);

CREATE TABLE term (
  term        TEXT NOT NULL,
  term_ja     TEXT CHECK (TRIM(term_ja) != ''),
  term_zh     TEXT CHECK (TRIM(term_zh) != ''),
  term_ko     TEXT CHECK (TRIM(term_ko) != ''),

  reading     TEXT CHECK (TRIM(reading) != ''),

  def_tags    TEXT CHECK (def_tags LIKE ' % '),  -- space-separated, accept NULL
  rules       TEXT CHECK (rules LIKE ' % '),
  score       INTEGER,
  -- glossary -- don't include in search table at all
  "sequence"  INTEGER,
  tags        TEXT CHECK (tags LIKE ' % '),

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
FROM term
WHERE def_tags IS NOT NULL OR rules IS NOT NULL OR tags IS NOT NULL;

CREATE VIRTUAL TABLE term_tags_fts USING fts5 (
  term        UNINDEXED,
  reading     UNINDEXED,
  def_tags,
  rules,
  tags,
  content='term_tags',
  tokenize="unicode61 tokenchars '-'"
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_tags_afer_insert AFTER INSERT ON term
WHEN new.def_tags IS NOT NULL OR new.rules IS NOT NULL OR new.tags IS NOT NULL
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
