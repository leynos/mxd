ALTER TABLE news_bundles
    ADD COLUMN guid TEXT,
    ADD COLUMN created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP;

UPDATE news_bundles
SET
    guid = COALESCE(guid, md5('news-bundle:' || id::text || ':' || clock_timestamp()::text)),
    created_at = COALESCE(created_at, CURRENT_TIMESTAMP);

ALTER TABLE news_categories
    DROP CONSTRAINT IF EXISTS news_categories_name_key,
    ADD COLUMN guid TEXT,
    ADD COLUMN add_sn INTEGER,
    ADD COLUMN delete_sn INTEGER,
    ADD COLUMN created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP;

CREATE INDEX IF NOT EXISTS idx_articles_category ON news_articles(category_id);

UPDATE news_categories AS c
SET
    guid = COALESCE(guid, md5('news-category:' || c.id::text || ':' || clock_timestamp()::text)),
    add_sn = COALESCE(
        add_sn,
        (
            SELECT COUNT(*)
            FROM news_articles AS a
            WHERE a.category_id = c.id
        )::INTEGER
    ),
    delete_sn = COALESCE(delete_sn, 0),
    created_at = COALESCE(created_at, CURRENT_TIMESTAMP);

ALTER TABLE news_categories
    DROP CONSTRAINT IF EXISTS news_categories_name_bundle_id_key;

CREATE UNIQUE INDEX idx_categories_name_bundle_unique
    ON news_categories(name, bundle_id)
    WHERE bundle_id IS NOT NULL;

CREATE UNIQUE INDEX idx_categories_root_name_unique
    ON news_categories(name)
    WHERE bundle_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_bundles_parent ON news_bundles(parent_bundle_id);
CREATE INDEX IF NOT EXISTS idx_bundles_name_parent ON news_bundles(name, parent_bundle_id);
CREATE INDEX IF NOT EXISTS idx_categories_bundle ON news_categories(bundle_id);
CREATE INDEX idx_articles_parent_article ON news_articles(parent_article_id);
CREATE INDEX idx_articles_prev_article ON news_articles(prev_article_id);
CREATE INDEX idx_articles_next_article ON news_articles(next_article_id);
CREATE INDEX idx_articles_first_child_article ON news_articles(first_child_article_id);
