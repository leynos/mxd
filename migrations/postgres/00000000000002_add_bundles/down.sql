DROP INDEX IF EXISTS idx_categories_bundle;
DROP INDEX IF EXISTS idx_bundles_parent;
ALTER TABLE news_categories DROP COLUMN bundle_id;
DROP TABLE news_bundles;
