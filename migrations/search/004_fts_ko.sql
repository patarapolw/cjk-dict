CREATE VIEW term_ko AS
SELECT
  term, reading, rowid,
  term_ko
FROM term
WHERE term_ko != '';

CREATE VIRTUAL TABLE term_ko_fts USING fts5 (
  term        UNINDEXED,
  reading     UNINDEXED,
  term_ko,
  content='term_ko'
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_ko_afer_insert AFTER INSERT ON term
WHEN new.term_ko != ''
BEGIN
  INSERT INTO term_ko_fts(
    term_ko,
  rowid) VALUES (
    new.term_ko,
  new.rowid);
END;
