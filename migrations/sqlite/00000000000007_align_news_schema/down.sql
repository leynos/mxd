DROP INDEX IF EXISTS idx_articles_first_child_article;
DROP INDEX IF EXISTS idx_articles_next_article;
DROP INDEX IF EXISTS idx_articles_prev_article;
DROP INDEX IF EXISTS idx_articles_parent_article;
DROP INDEX IF EXISTS idx_articles_category;
DROP INDEX IF EXISTS idx_news_categories_unique;
DROP INDEX IF EXISTS idx_categories_bundle;
DROP INDEX IF EXISTS idx_bundles_root_name_unique;
DROP INDEX IF EXISTS idx_bundles_name_parent;
DROP INDEX IF EXISTS idx_bundles_parent;
DROP INDEX IF EXISTS user_permissions_user_id_idx;
DROP INDEX IF EXISTS user_permissions_permission_id_idx;

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

CREATE TEMP TABLE news_category_downgrade_precondition (
    should_abort INTEGER NOT NULL
);

CREATE TEMP TRIGGER abort_duplicate_category_names_before_downgrade
BEFORE INSERT ON news_category_downgrade_precondition
WHEN NEW.should_abort = 1
BEGIN
    SELECT RAISE(ABORT, 'duplicate category names across bundles prevent downgrade');
END;

INSERT INTO news_category_downgrade_precondition (should_abort)
SELECT 1
WHERE EXISTS (
    SELECT 1
    FROM news_categories_new
    GROUP BY name
    HAVING COUNT(DISTINCT IFNULL(bundle_id, -1)) > 1
);

DROP TRIGGER abort_duplicate_category_names_before_downgrade;
DROP TABLE news_category_downgrade_precondition;

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
