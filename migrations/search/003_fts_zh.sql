CREATE VIEW term_zh AS
SELECT
  term, reading, rowid,
  term_zh
FROM term
WHERE term_zh != '';

CREATE VIRTUAL TABLE term_zh_fts USING fts5 (
  term        UNINDEXED,
  reading     UNINDEXED,
  term_zh,
  content='term_zh'
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_zh_afer_insert AFTER INSERT ON term
WHEN new.term_zh != ''
BEGIN
  INSERT INTO term_zh_fts(
    term_zh,
  rowid) VALUES (
    new.term_zh,
  new.rowid);
END;
