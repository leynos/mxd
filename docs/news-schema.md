# Database Schema Design for HXD News Functionality

The following is a compact, evidence-based design that maps Hotline’s hierarchical-news concepts (bundles → categories → articles, with threaded posts) and its granular privilege model into a relational schema.

---

```mermaid
erDiagram
    users {
        INTEGER id PK
        TEXT    username UNIQUE
        TEXT    password
    }

    permissions {
        INTEGER id PK
        INTEGER code        "numeric value from protocol"
        TEXT    name
        TEXT    scope       "general | folder | bundle"
    }

    user_permissions {
        INTEGER user_id FK
        INTEGER permission_id FK
        PRIMARY KEY (user_id, permission_id)
    }

    news_bundles {
        INTEGER id PK
        INTEGER parent_bundle_id FK "self-reference"
        TEXT    name
        TEXT    guid
        DATETIME created_at
    }

    news_categories {
        INTEGER id PK
        INTEGER bundle_id FK
        TEXT    name
        TEXT    guid
        INTEGER add_sn
        INTEGER delete_sn
        DATETIME created_at
    }

    news_articles {
        INTEGER id PK
        INTEGER category_id FK
        INTEGER parent_article_id FK
        INTEGER prev_article_id   FK
        INTEGER next_article_id   FK
        INTEGER first_child_article_id FK
        TEXT    title
        TEXT    poster
        DATETIME posted_at
        INTEGER flags
        TEXT    data_flavor
        TEXT    data
    }

    users           ||--o{ user_permissions : has
    permissions     ||--o{ user_permissions : grants
    news_bundles    ||--o{ news_bundles    : parent
    news_bundles    ||--o{ news_categories : contains
    news_categories ||--o{ news_articles   : contains
    news_articles   ||--|| news_articles   : prev_next
    news_articles   ||--o{ news_articles   : children
```

---

```sql
-- Users (given)
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    password TEXT NOT NULL
);

-- Discrete privilege catalogue
CREATE TABLE permissions (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    code    INTEGER NOT NULL,       -- matches protocol constant
    name    TEXT    NOT NULL,
    scope   TEXT    NOT NULL CHECK (scope IN ('general','folder','bundle')),
    UNIQUE(code)                    -- each protocol code appears once
);

-- User-to-privilege linking (many-to-many)
CREATE TABLE user_permissions (
    user_id       INTEGER NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
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

-- Categories live inside a bundle (or at top level if bundle_id IS NULL)
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

-- Articles support full threading and linear navigation
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

-- Helpful indices
CREATE INDEX idx_user_permissions_user  ON user_permissions(user_id);
CREATE INDEX idx_user_permissions_perm  ON user_permissions(permission_id);
CREATE INDEX idx_bundles_parent         ON news_bundles(parent_bundle_id);
CREATE INDEX idx_categories_bundle      ON news_categories(bundle_id);
CREATE INDEX idx_articles_category      ON news_articles(category_id);
```

### Why these tables?

* **Hierarchical news** – Bundles may contain bundles and categories, matching the server manual’s description of “bundles inside other bundles” and categories that hold posts .

  * Articles inherit Hotline’s thread metadata (parent/prev/next/first-child, flags, poster, date, flavour, data) as specified in the protocol fields list .
* **Permission model** – The protocol defines 38 distinct access flags (e.g. *News Read Article* code 20, *Broadcast* code 32) .  A lookup table plus an M-N link cleanly represents that bitmap while remaining normalised and queryable.
* **Users** – The provided `users` table is left intact and linked to permissions via `user_permissions`.

This schema stays portable (pure SQLite 3), enforces referential integrity via `ON DELETE CASCADE`, and leaves room for higher-level abstractions (roles, groups, moderation logs) without breaking existing contracts.
