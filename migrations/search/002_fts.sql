CREATE VIRTUAL TABLE term_fts USING fts5 (
  term        UNINDEXED,
  term_ja,
  term_zh,
  term_ko,

  reading,

  def_tags,
  rules,
  score       UNINDEXED,
  -- glossary -- don't include in search table at all
  "sequence"  UNINDEXED,
  tags,

  index_rowid     UNINDEXED,
  term_rowid      UNINDEXED,
  content='term',
  tokenize="unicode61 tokenchars '-'"
);

-- Triggers to keep the FTS index up to date.
-- Other triggers (after update/delete) not used, expect table to be rebuilt or left as is.
CREATE TRIGGER term_afer_insert AFTER INSERT ON term
BEGIN
  INSERT INTO term_fts(
    term_ja,
    term_zh,
    term_ko,
    reading,
    def_tags,
    rules,
    tags,
  rowid) VALUES (
    new.term_ja,
    new.term_zh,
    new.term_ko,
    new.reading,
    new.def_tags,
    new.rules,
    new.tags,
  new.rowid);
END;
