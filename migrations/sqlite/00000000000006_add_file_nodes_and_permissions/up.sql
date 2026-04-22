CREATE TABLE permissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    code INTEGER NOT NULL UNIQUE,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL
);

CREATE TABLE user_permissions (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission_id INTEGER NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, permission_id)
);

CREATE TABLE groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE user_groups (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, group_id)
);

CREATE TABLE file_nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('file', 'folder', 'alias')),
    name TEXT NOT NULL,
    parent_id INTEGER REFERENCES file_nodes(id) ON DELETE CASCADE,
    alias_target_id INTEGER REFERENCES file_nodes(id) ON DELETE RESTRICT,
    object_key TEXT,
    size INTEGER,
    comment TEXT,
    is_dropbox BOOLEAN NOT NULL DEFAULT 0,
    creator_id INTEGER NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (parent_id IS NULL OR parent_id <> id),
    CHECK (alias_target_id IS NULL OR alias_target_id <> id),
    CHECK (
        (kind = 'file'
         AND object_key IS NOT NULL
         AND alias_target_id IS NULL
         AND size IS NOT NULL
         AND is_dropbox = 0)
        OR
        (kind = 'folder'
         AND object_key IS NULL
         AND alias_target_id IS NULL
         AND size IS NULL)
        OR
        (kind = 'alias'
         AND object_key IS NULL
         AND alias_target_id IS NOT NULL
         AND size IS NULL
         AND is_dropbox = 0)
    )
);

CREATE UNIQUE INDEX idx_file_nodes_root_name
    ON file_nodes(name)
    WHERE parent_id IS NULL;

CREATE UNIQUE INDEX idx_file_nodes_child_name
    ON file_nodes(parent_id, name)
    WHERE parent_id IS NOT NULL;

CREATE UNIQUE INDEX idx_file_nodes_object_key
    ON file_nodes(object_key)
    WHERE object_key IS NOT NULL;

CREATE INDEX idx_file_nodes_parent_name
    ON file_nodes(parent_id, name);

CREATE INDEX idx_file_nodes_alias_target
    ON file_nodes(alias_target_id);

CREATE INDEX idx_file_nodes_creator
    ON file_nodes(creator_id);

CREATE TABLE resource_permissions (
    resource_type TEXT NOT NULL CHECK (resource_type = 'file_node'),
    resource_id INTEGER NOT NULL REFERENCES file_nodes(id) ON DELETE CASCADE,
    principal_type TEXT NOT NULL CHECK (principal_type IN ('user', 'group')),
    principal_id INTEGER NOT NULL,
    permission_id INTEGER NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (
        resource_type,
        resource_id,
        principal_type,
        principal_id,
        permission_id
    )
);

CREATE INDEX idx_resource_permissions_lookup
    ON resource_permissions(
        resource_type,
        principal_type,
        principal_id,
        permission_id,
        resource_id
    );

CREATE INDEX idx_resource_permissions_resource
    ON resource_permissions(resource_type, resource_id);

CREATE TRIGGER validate_resource_permissions_principal_insert
BEFORE INSERT ON resource_permissions
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.principal_type = 'user'
             AND NOT EXISTS (
                 SELECT 1 FROM users WHERE id = NEW.principal_id
             )
        THEN RAISE(ABORT, 'resource_permissions principal is not a valid user')
        WHEN NEW.principal_type = 'group'
             AND NOT EXISTS (
                 SELECT 1 FROM groups WHERE id = NEW.principal_id
             )
        THEN RAISE(ABORT, 'resource_permissions principal is not a valid group')
    END;
END;

CREATE TRIGGER validate_resource_permissions_principal_update
BEFORE UPDATE ON resource_permissions
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.principal_type = 'user'
             AND NOT EXISTS (
                 SELECT 1 FROM users WHERE id = NEW.principal_id
             )
        THEN RAISE(ABORT, 'resource_permissions principal is not a valid user')
        WHEN NEW.principal_type = 'group'
             AND NOT EXISTS (
                 SELECT 1 FROM groups WHERE id = NEW.principal_id
             )
        THEN RAISE(ABORT, 'resource_permissions principal is not a valid group')
    END;
END;

CREATE TRIGGER cleanup_resource_permissions_after_user_delete
AFTER DELETE ON users
FOR EACH ROW
BEGIN
    DELETE FROM resource_permissions
    WHERE principal_type = 'user'
      AND principal_id = OLD.id;
END;

CREATE TRIGGER cleanup_resource_permissions_after_group_delete
AFTER DELETE ON groups
FOR EACH ROW
BEGIN
    DELETE FROM resource_permissions
    WHERE principal_type = 'group'
      AND principal_id = OLD.id;
END;
