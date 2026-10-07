CREATE VIEW term_ko AS
SELECT
  rowid,
  index_rowid, term_rowid,  -- rowid columns help with JOIN
  term, reading,            -- just for quick debug term_[lang]
  term_ko
FROM term
WHERE term_ko IS NOT NULL;

CREATE VIRTUAL TABLE term_ko_fts USING fts5 (
  index_rowid UNINDEXED,
  term_rowid  UNINDEXED,
  term        UNINDEXED,
  reading     UNINDEXED,
  term_ko,
  content='term_ko'
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_ko_afer_insert AFTER INSERT ON term
WHEN new.term_ko IS NOT NULL
BEGIN
  INSERT INTO term_ko_fts(
    term_ko,
  rowid) VALUES (
    new.term_ko,
  new.rowid);
END;
