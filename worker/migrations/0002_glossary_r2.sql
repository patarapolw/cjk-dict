-- Glossaries too large for a D1 statement (100 KB) live in an R2 pack per dictionary
-- (dict.glossary_key); term.glossary is then NULL and (glossary_offset, glossary_size)
-- is the byte range of its JSON inside the pack.
ALTER TABLE dict ADD COLUMN glossary_key TEXT;
ALTER TABLE term ADD COLUMN glossary_offset INTEGER;
ALTER TABLE term ADD COLUMN glossary_size INTEGER;
