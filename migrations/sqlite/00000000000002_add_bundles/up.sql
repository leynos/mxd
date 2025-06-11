CREATE TABLE news_bundles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_bundle_id INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    UNIQUE(name, parent_bundle_id)
);

ALTER TABLE news_categories
    ADD COLUMN bundle_id INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE;

CREATE INDEX idx_bundles_parent ON news_bundles(parent_bundle_id);
CREATE INDEX idx_categories_bundle ON news_categories(bundle_id);
