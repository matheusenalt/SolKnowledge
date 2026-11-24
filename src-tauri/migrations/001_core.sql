CREATE TABLE profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE TABLE areas (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    profile_id TEXT REFERENCES profiles(id),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json))
);
CREATE TABLE modules (
    id TEXT PRIMARY KEY NOT NULL,
    area_id TEXT NOT NULL REFERENCES areas(id),
    parent_id TEXT,
    name TEXT NOT NULL,
    profile_id TEXT REFERENCES profiles(id),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    UNIQUE(id, area_id),
    FOREIGN KEY(parent_id, area_id) REFERENCES modules(id, area_id) DEFERRABLE INITIALLY DEFERRED,
    CHECK (parent_id IS NULL OR parent_id <> id)
);
CREATE TRIGGER modules_no_cycle_insert BEFORE INSERT ON modules
WHEN NEW.parent_id IS NOT NULL BEGIN
    SELECT RAISE(ABORT, 'Ciclo na hierarquia de modulos') WHERE NEW.id IN (
        WITH RECURSIVE ancestors(id, parent_id) AS (
            SELECT id, parent_id FROM modules WHERE id = NEW.parent_id
            UNION SELECT m.id, m.parent_id FROM modules m JOIN ancestors a ON m.id = a.parent_id
        ) SELECT id FROM ancestors UNION SELECT parent_id FROM ancestors
    );
END;
CREATE TRIGGER modules_no_cycle_update BEFORE UPDATE OF parent_id, id ON modules
WHEN NEW.parent_id IS NOT NULL BEGIN
    SELECT RAISE(ABORT, 'Ciclo na hierarquia de modulos') WHERE NEW.id IN (
        WITH RECURSIVE ancestors(id, parent_id) AS (
            SELECT id, parent_id FROM modules WHERE id = NEW.parent_id
            UNION SELECT m.id, m.parent_id FROM modules m JOIN ancestors a ON m.id = a.parent_id
        ) SELECT id FROM ancestors
    );
END;
CREATE TABLE entries (
    id TEXT PRIMARY KEY NOT NULL,
    module_id TEXT NOT NULL REFERENCES modules(id),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    title TEXT GENERATED ALWAYS AS (json_extract(payload_json, '$.title')) STORED NOT NULL,
    kind TEXT GENERATED ALWAYS AS (json_extract(payload_json, '$.type')) STORED NOT NULL
        CHECK(kind IN ('command','concept','summary','procedure','troubleshooting','lab','project','note','reference')),
    full_name TEXT GENERATED ALWAYS AS (coalesce(json_extract(payload_json, '$.full_name'), '')) STORED,
    options_coverage TEXT GENERATED ALWAYS AS (coalesce(json_extract(payload_json, '$.options_coverage'), 'selected')) STORED
        CHECK(options_coverage IN ('selected','complete')),
    learning_status TEXT GENERATED ALWAYS AS (coalesce(json_extract(payload_json, '$.learning_status'), 'not_assessed')) STORED
        CHECK(learning_status IN ('not_assessed','studying','understood','applied')),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
    CHECK (json_extract(payload_json, '$.id') IS NOT NULL AND json_extract(payload_json, '$.id') = id),
    CHECK (json_extract(payload_json, '$.module_id') IS NOT NULL AND json_extract(payload_json, '$.module_id') = module_id)
);
CREATE INDEX entries_module ON entries(module_id);
CREATE TABLE blocks (
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('text','personal_note','code','warning','reference','diagram','topology','image','attachment','equipment','cabling','procedure','troubleshooting','table','command')),
    position INTEGER NOT NULL CHECK(position >= 0),
    payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
    PRIMARY KEY(entry_id, id),
    CHECK(json_extract(payload_json, '$.id') IS NOT NULL AND json_extract(payload_json, '$.id') = id),
    CHECK(json_extract(payload_json, '$.type') IS NOT NULL AND json_extract(payload_json, '$.type') = kind)
);
CREATE TRIGGER blocks_immutable_type BEFORE UPDATE OF kind, id, entry_id ON blocks
WHEN NEW.kind <> OLD.kind OR NEW.id <> OLD.id OR NEW.entry_id <> OLD.entry_id BEGIN
    SELECT RAISE(ABORT, 'Identidade e tipo de bloco sao imutaveis');
END;
CREATE TABLE assets (
    id TEXT PRIMARY KEY NOT NULL,
    sha256 TEXT NOT NULL CHECK(length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    filename TEXT NOT NULL CHECK(length(filename) > 0 AND instr(filename, '/') = 0 AND instr(filename, char(92)) = 0 AND filename NOT IN ('.', '..')),
    media_type TEXT NOT NULL,
    size INTEGER NOT NULL CHECK(size >= 0)
);
CREATE TABLE favorites (
    entry_id TEXT PRIMARY KEY NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE search_history (
    id INTEGER PRIMARY KEY,
    query TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE import_history (
    package_id TEXT PRIMARY KEY NOT NULL,
    sha256 TEXT NOT NULL CHECK(length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    kind TEXT NOT NULL CHECK(kind IN ('seed','settings','knowledge_update')),
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL CHECK(json_valid(value_json))
);
CREATE TABLE synonyms (
    id INTEGER PRIMARY KEY,
    phrase TEXT NOT NULL,
    normalized TEXT NOT NULL,
    target_entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    UNIQUE(normalized, target_entry_id)
);
CREATE TABLE personal_vocabulary (
    id INTEGER PRIMARY KEY,
    phrase TEXT NOT NULL,
    normalized TEXT NOT NULL,
    target_entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    UNIQUE(normalized, target_entry_id)
);

-- All persisted block text stays searchable, including hidden components.
CREATE VIEW entry_search_content AS
SELECT e.id, e.title, e.full_name,
    coalesce((SELECT group_concat(text, ' ') FROM (
        SELECT CAST(atom AS TEXT) AS text FROM json_tree(e.payload_json)
        WHERE type = 'text' AND key NOT IN ('id','module_id','type','learning_status','options_coverage')
        UNION ALL
        SELECT CAST(j.atom AS TEXT) FROM blocks b, json_tree(b.payload_json) j
        WHERE b.entry_id = e.id AND j.type = 'text' AND j.key NOT IN ('id','type')
    )), '') AS body
FROM entries e;
CREATE VIRTUAL TABLE entries_fts USING fts5(
    entry_id UNINDEXED, title, full_name, body,
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER entries_fts_insert AFTER INSERT ON entries BEGIN
    INSERT INTO entries_fts(entry_id, title, full_name, body)
    SELECT id, title, full_name, body FROM entry_search_content WHERE id = NEW.id;
END;
CREATE TRIGGER entries_fts_update AFTER UPDATE ON entries BEGIN
    DELETE FROM entries_fts WHERE entry_id = OLD.id;
    INSERT INTO entries_fts(entry_id, title, full_name, body)
    SELECT id, title, full_name, body FROM entry_search_content WHERE id = NEW.id;
END;
CREATE TRIGGER entries_fts_delete AFTER DELETE ON entries BEGIN
    DELETE FROM entries_fts WHERE entry_id = OLD.id;
END;
CREATE TRIGGER blocks_fts_insert AFTER INSERT ON blocks BEGIN
    DELETE FROM entries_fts WHERE entry_id = NEW.entry_id;
    INSERT INTO entries_fts(entry_id, title, full_name, body)
    SELECT id, title, full_name, body FROM entry_search_content WHERE id = NEW.entry_id;
END;
CREATE TRIGGER blocks_fts_update AFTER UPDATE ON blocks BEGIN
    DELETE FROM entries_fts WHERE entry_id = NEW.entry_id;
    INSERT INTO entries_fts(entry_id, title, full_name, body)
    SELECT id, title, full_name, body FROM entry_search_content WHERE id = NEW.entry_id;
END;
CREATE TRIGGER blocks_fts_delete AFTER DELETE ON blocks BEGIN
    DELETE FROM entries_fts WHERE entry_id = OLD.entry_id;
    INSERT INTO entries_fts(entry_id, title, full_name, body)
    SELECT id, title, full_name, body FROM entry_search_content WHERE id = OLD.entry_id;
END;
