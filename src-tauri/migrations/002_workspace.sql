ALTER TABLE areas ADD COLUMN revision INTEGER NOT NULL DEFAULT 1;
ALTER TABLE modules ADD COLUMN revision INTEGER NOT NULL DEFAULT 1;
CREATE TABLE visit_history (
    id INTEGER PRIMARY KEY,
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    visited_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX visit_history_recent ON visit_history(id DESC);
CREATE INDEX search_history_recent ON search_history(id DESC);
