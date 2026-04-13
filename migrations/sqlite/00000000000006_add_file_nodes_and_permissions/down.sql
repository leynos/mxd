DROP INDEX IF EXISTS idx_resource_permissions_principal_resource;
DROP TABLE IF EXISTS resource_permissions;

DROP INDEX IF EXISTS idx_file_nodes_alias_target;
DROP INDEX IF EXISTS idx_file_nodes_parent_name;
DROP INDEX IF EXISTS idx_file_nodes_single_root;
DROP TABLE IF EXISTS file_nodes;

DROP INDEX IF EXISTS idx_user_permissions_permission_user;
DROP TABLE IF EXISTS user_permissions;
DROP TABLE IF EXISTS permissions;
