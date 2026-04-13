DROP INDEX IF EXISTS idx_user_permissions_perm;
DROP INDEX IF EXISTS idx_user_permissions_user;
DROP TABLE IF EXISTS user_permissions;
DROP TABLE IF EXISTS permissions;

DROP INDEX IF EXISTS idx_articles_first_child_article;
DROP INDEX IF EXISTS idx_articles_next_article;
DROP INDEX IF EXISTS idx_articles_prev_article;
DROP INDEX IF EXISTS idx_articles_parent_article;
DROP INDEX IF EXISTS idx_articles_category;
DROP INDEX IF EXISTS idx_categories_bundle;
DROP INDEX IF EXISTS idx_bundles_name_parent;
DROP INDEX IF EXISTS idx_bundles_parent;

ALTER TABLE news_articles RENAME TO news_articles_new;
ALTER TABLE news_categories RENAME TO news_categories_new;
ALTER TABLE news_bundles RENAME TO news_bundles_new;

CREATE TABLE news_bundles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_bundle_id INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    UNIQUE(name, parent_bundle_id)
);

INSERT INTO news_bundles (id, parent_bundle_id, name)
SELECT id, parent_bundle_id, name
FROM news_bundles_new;

CREATE INDEX idx_bundles_parent ON news_bundles(parent_bundle_id);
CREATE INDEX idx_bundles_name_parent ON news_bundles(name, parent_bundle_id);

CREATE TABLE news_categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    bundle_id INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE
);

INSERT INTO news_categories (id, name, bundle_id)
SELECT id, name, bundle_id
FROM news_categories_new;

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
FROM news_articles_new;

CREATE INDEX idx_articles_category ON news_articles(category_id);

DROP TABLE news_articles_new;
DROP TABLE news_categories_new;
DROP TABLE news_bundles_new;
