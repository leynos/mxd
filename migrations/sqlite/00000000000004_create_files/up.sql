CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    object_key TEXT NOT NULL,
    size INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE file_acl (
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (file_id, user_id)
);

CREATE INDEX idx_file_acl_user_file ON file_acl (user_id, file_id);
