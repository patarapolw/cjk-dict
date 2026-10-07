CREATE VIEW term_ja AS
SELECT
  rowid,
  index_rowid, term_rowid,  -- rowid columns help with JOIN
  term, reading,            -- just for quick debug term_[lang]
  term_ja
FROM term
WHERE term_ja IS NOT NULL;

CREATE VIRTUAL TABLE term_ja_fts USING fts5 (
  index_rowid UNINDEXED,
  term_rowid  UNINDEXED,
  term        UNINDEXED,
  reading     UNINDEXED,
  term_ja,
  content='term_ja'
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_ja_afer_insert AFTER INSERT ON term
WHEN new.term_ja IS NOT NULL
BEGIN
  INSERT INTO term_ja_fts(
    term_ja,
  rowid) VALUES (
    new.term_ja,
  new.rowid);
END;
