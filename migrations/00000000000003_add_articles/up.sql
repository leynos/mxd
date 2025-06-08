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

CREATE INDEX idx_articles_category ON news_articles(category_id);
