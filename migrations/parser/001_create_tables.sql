-- CREATE TABLE glossaries (
--     "id"            INTEGER PRIMARY KEY,
--     "hash"          TEXT UNIQUE,                -- create hash if unique content needs to be enforced
--     "content"       TEXT NOT NULL
-- );

CREATE TABLE kanji (
    kanji           TEXT NOT NULL,
    onyomi          TEXT,
    kunyomi         TEXT,
    tags            TEXT,
    meanings        TEXT NOT NULL DEFAULT '[]',
    "stats"         TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (kanji)
);
CREATE TABLE kanji_meta (
    kanji           TEXT NOT NULL,      -- not set primary key, allow duplicates
    "type"          TEXT NOT NULL,      -- 'freq' correspond to frequency information
    "data"          INTEGER NOT NULL    -- probably TEXT, but make use of SQLite affinity for type=freq
);

CREATE TABLE tag (
    "name"          TEXT NOT NULL,
    category        TEXT,
    sort_order      INTEGER,
    notes           TEXT,
    score           INTEGER,
    PRIMARY KEY ("name")
);

CREATE TABLE term (
    term            TEXT NOT NULL,
    reading         TEXT NOT NULL,
    def_tags        TEXT,
    rules           TEXT,
    score           INTEGER,
    glossary        TEXT,
    "sequence"      INTEGER,
    tags            TEXT
);
CREATE TABLE term_meta (
    term            TEXT NOT NULL,
    "type"          TEXT NOT NULL,
    "data"          TEXT NOT NULL
);
