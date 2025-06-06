-- Discrete privilege catalogue
CREATE TABLE permissions (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    code    INTEGER NOT NULL,
    name    TEXT    NOT NULL,
    scope   TEXT    NOT NULL CHECK (scope IN ('general','folder','bundle')),
    UNIQUE(code)
);

-- User-to-privilege linking
CREATE TABLE user_permissions (
    user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission_id INTEGER NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, permission_id)
);

-- Bundles can nest recursively
CREATE TABLE news_bundles (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_bundle_id INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE,
    name             TEXT    NOT NULL,
    guid             TEXT,
    created_at       DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(name, parent_bundle_id)
);

-- Categories inside a bundle
CREATE TABLE news_categories (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    bundle_id  INTEGER REFERENCES news_bundles(id) ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    guid       TEXT,
    add_sn     INTEGER,
    delete_sn  INTEGER,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(name, bundle_id)
);

-- Articles with threading
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

CREATE INDEX idx_user_permissions_user  ON user_permissions(user_id);
CREATE INDEX idx_user_permissions_perm  ON user_permissions(permission_id);
CREATE INDEX idx_bundles_parent         ON news_bundles(parent_bundle_id);
CREATE INDEX idx_categories_bundle      ON news_categories(bundle_id);
CREATE INDEX idx_articles_category      ON news_articles(category_id);
