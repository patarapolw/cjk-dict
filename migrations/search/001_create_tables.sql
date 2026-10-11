CREATE TABLE "index" (
  "path"        TEXT NOT NULL PRIMARY KEY,  -- actual YomitanReader root_dir

  L1            TEXT,                       -- data[$.sourceLanguage] or enforced by user
  L2            TEXT,                       -- data[$.targetLanguage] or enforced by user
  "data"        TEXT_JSON NOT NULL
);

CREATE TABLE term_search (
  term        TEXT NOT NULL,
  term_ja     TEXT CHECK (term_ja   LIKE ' %_ '),
  term_zh     TEXT CHECK (term_zh   LIKE ' %_ '),
  term_ko     TEXT CHECK (term_ko   LIKE ' %_ '),

  reading     TEXT CHECK (reading   LIKE ' %_ '), -- could be helpful for ' Pin[1-5] Yin[1-5] '

  def_tags    TEXT CHECK (def_tags  LIKE ' %_ '), -- space-separated, accept NULL
  rules       TEXT CHECK (rules     LIKE ' %_ '),
  score       INTEGER,

  tags        TEXT CHECK (tags      LIKE ' %_ '),

  "ref"       TEXT_JSON NOT NULL                  -- { dict_id: u64, term_id: u64, score: int | null }[]
);

CREATE UNIQUE INDEX idx_u_term_search ON term_search (
  term, reading,
  (term_ja IS NULL), (term_zh IS NULL), (term_ko IS NULL)
);
