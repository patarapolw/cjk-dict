CREATE VIEW term_zh AS
SELECT
  rowid,
  index_rowid, term_rowid,  -- rowid columns help with JOIN
  term, reading,            -- just for quick debug term_[lang]
  term_zh
FROM term
WHERE term_zh IS NOT NULL;

CREATE VIRTUAL TABLE term_zh_fts USING fts5 (
  index_rowid UNINDEXED,
  term_rowid  UNINDEXED,
  term        UNINDEXED,
  reading     UNINDEXED,
  term_zh,
  content='term_zh'
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_zh_afer_insert AFTER INSERT ON term
WHEN new.term_zh IS NOT NULL
BEGIN
  INSERT INTO term_zh_fts(
    term_zh,
  rowid) VALUES (
    new.term_zh,
  new.rowid);
END;
