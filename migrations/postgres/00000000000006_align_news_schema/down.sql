DROP INDEX IF EXISTS idx_user_permissions_perm;
DROP INDEX IF EXISTS idx_user_permissions_user;
DROP TABLE IF EXISTS user_permissions;
DROP TABLE IF EXISTS permissions;

DROP INDEX IF EXISTS idx_articles_first_child_article;
DROP INDEX IF EXISTS idx_articles_next_article;
DROP INDEX IF EXISTS idx_articles_prev_article;
DROP INDEX IF EXISTS idx_articles_parent_article;
DROP INDEX IF EXISTS idx_categories_root_name_unique;

ALTER TABLE news_categories
    DROP CONSTRAINT IF EXISTS news_categories_name_bundle_id_key;

ALTER TABLE news_categories
    DROP COLUMN IF EXISTS created_at,
    DROP COLUMN IF EXISTS delete_sn,
    DROP COLUMN IF EXISTS add_sn,
    DROP COLUMN IF EXISTS guid;

ALTER TABLE news_bundles
    DROP COLUMN IF EXISTS created_at,
    DROP COLUMN IF EXISTS guid;
