DROP INDEX IF EXISTS idx_articles_first_child_article;
DROP INDEX IF EXISTS idx_articles_category;
DROP INDEX IF EXISTS idx_articles_next_article;
DROP INDEX IF EXISTS idx_articles_prev_article;
DROP INDEX IF EXISTS idx_articles_parent_article;
DROP INDEX IF EXISTS idx_bundles_parent;
DROP INDEX IF EXISTS idx_bundles_name_parent;
DROP INDEX IF EXISTS idx_bundles_root_name_unique;
DROP INDEX IF EXISTS idx_categories_bundle;
DROP INDEX IF EXISTS user_permissions_user_id_idx;
DROP INDEX IF EXISTS user_permissions_permission_id_idx;
DROP INDEX IF EXISTS idx_categories_name_bundle_unique;
DROP INDEX IF EXISTS idx_categories_root_name_unique;

ALTER TABLE news_categories
    DROP CONSTRAINT IF EXISTS news_categories_name_bundle_id_key;

ALTER TABLE news_categories
    DROP COLUMN IF EXISTS created_at,
    DROP COLUMN IF EXISTS delete_sn,
    DROP COLUMN IF EXISTS add_sn,
    DROP COLUMN IF EXISTS guid;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM news_categories
        GROUP BY name
        HAVING COUNT(*) > 1
    ) THEN
        RAISE EXCEPTION 'duplicate category names across bundles prevent downgrade';
    END IF;
END $$;

ALTER TABLE news_categories
    ADD CONSTRAINT news_categories_name_key UNIQUE (name);

ALTER TABLE news_bundles
    DROP COLUMN IF EXISTS created_at,
    DROP COLUMN IF EXISTS guid;
