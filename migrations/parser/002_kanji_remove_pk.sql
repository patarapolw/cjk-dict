ALTER TABLE kanji RENAME TO kanji_old;

CREATE TABLE kanji (
    kanji           TEXT NOT NULL,
    onyomi          TEXT,
    kunyomi         TEXT,
    tags            TEXT,
    meanings        TEXT NOT NULL DEFAULT '[]',
    "stats"         TEXT NOT NULL DEFAULT '{}'
);

INSERT INTO kanji
SELECT * FROM kanji_old;

DROP TABLE kanji_old;
