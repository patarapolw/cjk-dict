CREATE TABLE "index" (
  "path"        TEXT NOT NULL PRIMARY KEY,  -- actual YomitanReader root_dir

  L1            TEXT NOT NULL, --data[$.sourceLanguage] ensure not missing
  L2            TEXT NOT NULL, --data[$.targetLanguage] ensure not missing
  "data"        TEXT_JSON NOT NULL
);

CREATE TABLE term (
  term        TEXT NOT NULL,
  term_parsed_ja      TEXT,
  term_parsed_zh      TEXT,
  term_parsed_ko      TEXT,

  reading     TEXT NOT NULL,
  reading_parsed_ja   TEXT,
  reading_parsed_zh   TEXT,

  def_tags    TEXT, -- space-separated
  rules       TEXT, -- space-separated
  score       INTEGER,
  -- glossary -- don't include in search table at all
  "sequence"  INTEGER,
  tags        TEXT, -- space-separated

  L1          TEXT NOT NULL, -- index.L1
  L2          TEXT NOT NULL, -- index.L2
  index_rowid INTEGER NOT NULL REFERENCES "index" (rowid)
);

CREATE INDEX idx_term_score ON term (score);
CREATE INDEX idx_term_sequence ON term ("sequence");

CREATE VIRTUAL TABLE term_fts USING fts5 (
  term,
  term_parsed_ja, -- VIRTUAL TABLE will need to dropped and recreated for additional language support.
  term_parsed_zh,
  term_parsed_ko,
  reading,
  reading_parsed_ja,
  reading_parsed_zh,
  def_tags,
  rules,
  score       UNINDEXED,  -- consider sorting non-TEXT outside
  "sequence"  UNINDEXED,
  tags,
  L1,
  L2,
  index_rowid UNINDEXED,
  content='term'
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_afer_insert AFTER INSERT ON term BEGIN
  INSERT INTO term_fts(
    term,
    term_parsed_ja,
    term_parsed_zh,
    term_parsed_ko,
    reading,
    reading_parsed_ja,
    reading_parsed_zh,
    def_tags,
    rules,
    tags,
    L1,
    L2,
  rowid) VALUES (
    new.term,
    new.term_parsed_ja,
    new.term_parsed_zh,
    new.term_parsed_ko,
    new.reading,
    new.reading_parsed_ja,
    new.reading_parsed_zh,
    new.def_tags,
    new.rules,
    new.tags,
    new.L1,
    new.L2,
  new.rowid);
END;
