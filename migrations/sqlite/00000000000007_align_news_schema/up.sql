DROP INDEX IF EXISTS idx_categories_bundle;
DROP INDEX IF EXISTS idx_bundles_name_parent;
DROP INDEX IF EXISTS idx_bundles_parent;

ALTER TABLE news_articles RENAME TO news_articles_old;
ALTER TABLE news_categories RENAME TO news_categories_old;
ALTER TABLE news_bundles RENAME TO news_bundles_old;

CREATE TABLE news_bundles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_bundle_id INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    guid TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(name, parent_bundle_id)
);

CREATE TRIGGER news_bundles_guid_after_insert
AFTER INSERT ON news_bundles
WHEN NEW.guid IS NULL
BEGIN
    UPDATE news_bundles
    SET guid = lower(hex(randomblob(16)))
    WHERE id = NEW.id;
END;

INSERT INTO news_bundles (id, parent_bundle_id, name, guid, created_at)
SELECT
    id,
    parent_bundle_id,
    name,
    lower(hex(randomblob(16))),
    CURRENT_TIMESTAMP
FROM news_bundles_old;

CREATE INDEX idx_bundles_parent ON news_bundles(parent_bundle_id);
CREATE INDEX idx_bundles_name_parent ON news_bundles(name, parent_bundle_id);

CREATE TABLE news_categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bundle_id INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    guid TEXT,
    add_sn INTEGER DEFAULT 0,
    delete_sn INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER news_categories_guid_after_insert
AFTER INSERT ON news_categories
WHEN NEW.guid IS NULL
BEGIN
    UPDATE news_categories
    SET guid = lower(hex(randomblob(16)))
    WHERE id = NEW.id;
END;

CREATE UNIQUE INDEX idx_news_categories_unique
    ON news_categories(name, IFNULL(bundle_id, -1));

INSERT INTO news_categories (id, bundle_id, name, guid, add_sn, delete_sn, created_at)
SELECT
    c.id,
    c.bundle_id,
    c.name,
    lower(hex(randomblob(16))),
    (SELECT COUNT(*) FROM news_articles_old a WHERE a.category_id = c.id),
    0,
    CURRENT_TIMESTAMP
FROM news_categories_old c;

-- Keep the legacy article/category index available for the correlated
-- add_sn backfill above, then drop it before recreating the index on the
-- replacement news_articles table.
DROP INDEX IF EXISTS idx_articles_category;

CREATE INDEX idx_categories_bundle ON news_categories(bundle_id);

CREATE TABLE news_articles (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    category_id            INTEGER NOT NULL REFERENCES news_categories(id) ON DELETE CASCADE,
    parent_article_id      INTEGER REFERENCES news_articles(id),
    prev_article_id        INTEGER REFERENCES news_articles(id),
    next_article_id        INTEGER REFERENCES news_articles(id),
    first_child_article_id INTEGER REFERENCES news_articles(id),
    title       TEXT    NOT NULL,
    poster      TEXT,
    posted_at   DATETIME NOT NULL,
    flags       INTEGER DEFAULT 0,
    data_flavor TEXT    DEFAULT 'text/plain',
    data        TEXT,
    CHECK (category_id IS NOT NULL)
);

INSERT INTO news_articles (
    id,
    category_id,
    parent_article_id,
    prev_article_id,
    next_article_id,
    first_child_article_id,
    title,
    poster,
    posted_at,
    flags,
    data_flavor,
    data
)
SELECT
    id,
    category_id,
    parent_article_id,
    prev_article_id,
    next_article_id,
    first_child_article_id,
    title,
    poster,
    posted_at,
    flags,
    data_flavor,
    data
FROM news_articles_old;

CREATE INDEX idx_articles_category ON news_articles(category_id);
CREATE INDEX idx_articles_parent_article ON news_articles(parent_article_id);
CREATE INDEX idx_articles_prev_article ON news_articles(prev_article_id);
CREATE INDEX idx_articles_next_article ON news_articles(next_article_id);
CREATE INDEX idx_articles_first_child_article ON news_articles(first_child_article_id);

DROP TABLE news_articles_old;
DROP TABLE news_categories_old;
DROP TABLE news_bundles_old;
