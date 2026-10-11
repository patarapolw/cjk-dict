CREATE VIRTUAL TABLE term_search_fts USING fts5 (
  "term"  UNINDEXED,
  "term_ja",
  "term_zh",
  "term_ko",
  "reading",
  "def_tags",
  "rules",
  "score"  UNINDEXED,
  "tags",
  "ref"  UNINDEXED,
  content='term_search',
  tokenize="unicode61 tokenchars '-'"
);

CREATE TRIGGER t_term_search_after_insert
AFTER INSERT ON "term_search"
BEGIN
  INSERT INTO term_search_fts (
    "term_ja","term_zh","term_ko","reading","def_tags","rules","tags",
    rowid
  ) VALUES (
    new.term_ja,new.term_zh,new.term_ko,new.reading,new.def_tags,new.rules,new.tags,
    new.rowid
  );
END;

CREATE TRIGGER t_term_search_after_delete
AFTER DELETE ON "term_search"
BEGIN
  INSERT INTO term_search_fts (
    "term_ja","term_zh","term_ko","reading","def_tags","rules","tags",
    rowid, term_search_fts
  ) VALUES (
    old.term_ja,old.term_zh,old.term_ko,old.reading,old.def_tags,old.rules,old.tags,
    old.rowid, 'delete'
  );
END;

CREATE TRIGGER t_term_search_after_update
AFTER UPDATE ON "term_search"
BEGIN
  INSERT INTO term_search_fts (
    "term_ja","term_zh","term_ko","reading","def_tags","rules","tags",
    rowid, term_search_fts
  ) VALUES (
    old.term_ja,old.term_zh,old.term_ko,old.reading,old.def_tags,old.rules,old.tags,
    old.rowid, 'delete'
  );

  INSERT INTO term_search_fts (
    "term_ja","term_zh","term_ko","reading","def_tags","rules","tags",
    rowid
  ) VALUES (
    new.term_ja,new.term_zh,new.term_ko,new.reading,new.def_tags,new.rules,new.tags,
    new.rowid
  );
END;
