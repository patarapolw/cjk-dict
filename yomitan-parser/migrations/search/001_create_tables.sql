CREATE TABLE "index" (
  id          INTEGER PRIMARY KEY,
  "path"      TEXT NOT NULL,  -- actual YomitanReader root_dir

  L1          TEXT,           -- data[$.sourceLanguage] or enforced by user
  L2          TEXT,           -- data[$.targetLanguage] or enforced by user
  "data"      TEXT_JSON,      -- originally for index.json
  UNIQUE ("path")
);

CREATE TABLE term_search (
  id          INTEGER PRIMARY KEY,
  term        TEXT NOT NULL,
  term_ja     TEXT NOT NULL DEFAULT '', -- space-separated, unique check
  term_zh     TEXT NOT NULL DEFAULT '', -- space-separated, unique check
  term_ko     TEXT NOT NULL DEFAULT '', -- space-separated, unique check

  reading     TEXT NOT NULL DEFAULT '' CHECK (reading = '' OR reading LIKE ' %_ '), -- could be helpful for ' Pin[1-5] Yin[1-5] '

  def_tags    TEXT, -- space-separated, accept NULL
  rules       TEXT, -- space-separated, accept NULL
  score       INTEGER,  -- max score

  tags        TEXT, -- space-separated, accept NULL

  "data"      TEXT_JSON   -- for adding no-SQL without changing schema
);

CREATE UNIQUE INDEX idx_u_term_search ON term_search (
  term, reading,
  (term_ja != ''), (term_zh != ''), (term_ko != '')
);

CREATE INDEX idx_term_search_score ON term_search (score);

CREATE TABLE term_link (
  dict_id     INTEGER NOT NULL REFERENCES "index" (id) ON DELETE CASCADE,
  search_id   INTEGER NOT NULL REFERENCES term_search (id),
  term_id     INTEGER NOT NULL,   -- REFERENCES dict[i].term (rowid)
  score       INTEGER,            -- actual score
  "data"      TEXT_JSON,          -- for adding no-SQL without changing schema
  PRIMARY KEY (dict_id, term_id)  -- prevent from adding the same term twice
) WITHOUT rowid;

CREATE INDEX idx_term_link_search_id ON term_link (search_id);
