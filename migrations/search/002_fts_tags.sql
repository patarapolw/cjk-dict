CREATE VIEW term_tags AS
SELECT
  rowid,
  index_rowid, term_rowid,  -- rowid columns help with JOIN
  term, reading,            -- just for quick debug term_[lang]
  def_tags, rules, tags
FROM term
WHERE def_tags IS NOT NULL OR rules IS NOT NULL OR tags IS NOT NULL;

CREATE VIRTUAL TABLE term_tags_fts USING fts5 (
  index_rowid UNINDEXED,
  term_rowid  UNINDEXED,
  term        UNINDEXED,
  reading     UNINDEXED,
  def_tags,
  rules,
  tags,
  content='term_tags',
  tokenize="unicode61 tokenchars '-'"
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_tags_afer_insert AFTER INSERT ON term
WHEN new.def_tags IS NOT NULL OR new.rules IS NOT NULL OR new.tags IS NOT NULL
BEGIN
  INSERT INTO term_tags_fts(
    def_tags,
    rules,
    tags,
  rowid) VALUES (
    new.def_tags,
    new.rules,
    new.tags,
  new.rowid);
END;
