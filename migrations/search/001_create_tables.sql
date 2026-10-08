CREATE TABLE "index" (
  "path"        TEXT NOT NULL PRIMARY KEY,  -- actual YomitanReader root_dir

  L1            TEXT,                       -- data[$.sourceLanguage] or enforced by user
  L2            TEXT,                       -- data[$.targetLanguage] or enforced by user
  "data"        TEXT_JSON NOT NULL
);

CREATE TABLE term (
  term        TEXT NOT NULL,
  term_ja     TEXT CHECK (term_ja   LIKE ' %_ '),
  term_zh     TEXT CHECK (term_zh   LIKE ' %_ '),
  term_ko     TEXT CHECK (term_ko   LIKE ' %_ '),

  reading     TEXT CHECK (reading   LIKE ' %_ '), -- could be helpful for ' Pin[1-5] Yin[1-5] '

  def_tags    TEXT CHECK (def_tags  LIKE ' %_ '), -- space-separated, accept NULL
  rules       TEXT CHECK (rules     LIKE ' %_ '),
  score       INTEGER,
  -- glossary -- don't include in search table at all
  "sequence"  INTEGER,
  tags        TEXT CHECK (tags      LIKE ' %_ '),

  index_rowid     INTEGER NOT NULL REFERENCES "index" (rowid) ON DELETE CASCADE,
  term_rowid      INTEGER NOT NULL  -- REFERENCES dicts[i].term (rowid)
);

CREATE INDEX idx_term_term ON term (term);
CREATE INDEX idx_term_reading ON term (reading);
CREATE INDEX idx_term_score ON term (score);
CREATE INDEX idx_term_sequence ON term ("sequence");

CREATE INDEX idx_term_index_rowid ON term (index_rowid);
