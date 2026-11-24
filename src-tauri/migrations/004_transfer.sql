ALTER TABLE import_history ADD COLUMN hash_format TEXT NOT NULL DEFAULT 'raw' CHECK(hash_format IN ('raw','canonical-v1'));
