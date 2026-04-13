CREATE TABLE permissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    code INTEGER NOT NULL UNIQUE,
    name TEXT NOT NULL UNIQUE,
    scope TEXT NOT NULL CHECK (scope IN ('general', 'file', 'news'))
);

INSERT INTO permissions (code, name, scope) VALUES
    (0, 'delete_file', 'file'),
    (1, 'upload_file', 'file'),
    (2, 'download_file', 'file'),
    (3, 'rename_file', 'file'),
    (4, 'move_file', 'file'),
    (5, 'create_folder', 'file'),
    (6, 'delete_folder', 'file'),
    (7, 'rename_folder', 'file'),
    (8, 'move_folder', 'file'),
    (9, 'read_chat', 'general'),
    (10, 'send_chat', 'general'),
    (11, 'open_chat', 'general'),
    (12, 'close_chat', 'general'),
    (13, 'show_in_list', 'general'),
    (14, 'create_user', 'general'),
    (15, 'delete_user', 'general'),
    (16, 'open_user', 'general'),
    (17, 'modify_user', 'general'),
    (18, 'change_own_password', 'general'),
    (19, 'send_private_message', 'general'),
    (20, 'news_read_article', 'news'),
    (21, 'news_post_article', 'news'),
    (22, 'disconnect_user', 'general'),
    (23, 'cannot_be_disconnected', 'general'),
    (24, 'get_client_info', 'general'),
    (25, 'upload_anywhere', 'file'),
    (26, 'any_name', 'general'),
    (27, 'no_agreement', 'general'),
    (28, 'set_file_comment', 'file'),
    (29, 'set_folder_comment', 'file'),
    (30, 'view_drop_boxes', 'file'),
    (31, 'make_alias', 'file'),
    (32, 'broadcast', 'general'),
    (33, 'news_delete_article', 'news'),
    (34, 'news_create_category', 'news'),
    (35, 'news_delete_category', 'news'),
    (36, 'news_create_folder', 'news'),
    (37, 'news_delete_folder', 'news');

CREATE TABLE user_permissions (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission_id INTEGER NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, permission_id)
);

CREATE INDEX idx_user_permissions_permission_user
    ON user_permissions (permission_id, user_id);

CREATE TABLE file_nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    is_root BOOLEAN NOT NULL DEFAULT FALSE,
    node_type TEXT NOT NULL CHECK (node_type IN ('file', 'folder', 'alias')),
    name TEXT NOT NULL,
    parent_id INTEGER REFERENCES file_nodes(id) ON DELETE CASCADE,
    alias_target_id INTEGER REFERENCES file_nodes(id) ON DELETE RESTRICT,
    object_key TEXT,
    size INTEGER NOT NULL DEFAULT 0 CHECK (size >= 0),
    comment TEXT,
    is_dropbox BOOLEAN NOT NULL DEFAULT FALSE,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    CHECK (
        (is_root = 1 AND node_type = 'folder' AND parent_id IS NULL AND alias_target_id IS NULL
         AND object_key IS NULL AND size = 0 AND is_dropbox = 0)
        OR
        (is_root = 0 AND parent_id IS NOT NULL)
    ),
    CHECK (
        (node_type = 'file' AND alias_target_id IS NULL AND object_key IS NOT NULL
         AND is_dropbox = 0)
        OR
        (node_type = 'folder' AND alias_target_id IS NULL AND object_key IS NULL AND size = 0)
        OR
        (node_type = 'alias' AND alias_target_id IS NOT NULL AND object_key IS NULL
         AND size = 0 AND is_dropbox = 0)
    ),
    CHECK (alias_target_id IS NULL OR alias_target_id <> id),
    UNIQUE (parent_id, name)
);

CREATE UNIQUE INDEX idx_file_nodes_single_root
    ON file_nodes (is_root)
    WHERE is_root = 1;

CREATE INDEX idx_file_nodes_parent_name
    ON file_nodes (parent_id, name);

CREATE INDEX idx_file_nodes_alias_target
    ON file_nodes (alias_target_id)
    WHERE alias_target_id IS NOT NULL;

INSERT INTO file_nodes (is_root, node_type, name, size, is_dropbox)
VALUES (TRUE, 'folder', '', 0, FALSE);

CREATE TABLE resource_permissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    resource_type TEXT NOT NULL CHECK (resource_type = 'file_node'),
    resource_id INTEGER NOT NULL REFERENCES file_nodes(id) ON DELETE CASCADE,
    principal_type TEXT NOT NULL CHECK (principal_type = 'user'),
    principal_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    privileges BIGINT NOT NULL CHECK (privileges >= 0),
    UNIQUE (resource_type, resource_id, principal_type, principal_id)
);

CREATE INDEX idx_resource_permissions_principal_resource
    ON resource_permissions (principal_type, principal_id, resource_type, resource_id);
