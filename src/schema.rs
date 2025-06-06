diesel::table! {
    users (id) {
        id -> Integer,
        username -> Text,
        password -> Text,
    }
}

diesel::table! {
    permissions (id) {
        id -> Integer,
        code -> Integer,
        name -> Text,
        scope -> Text,
    }
}

diesel::table! {
    user_permissions (user_id, permission_id) {
        user_id -> Integer,
        permission_id -> Integer,
    }
}

diesel::table! {
    news_bundles (id) {
        id -> Integer,
        parent_bundle_id -> Nullable<Integer>,
        name -> Text,
        guid -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    news_categories (id) {
        id -> Integer,
        bundle_id -> Nullable<Integer>,
        name -> Text,
        guid -> Nullable<Text>,
        add_sn -> Nullable<Integer>,
        delete_sn -> Nullable<Integer>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    news_articles (id) {
        id -> Integer,
        category_id -> Integer,
        parent_article_id -> Nullable<Integer>,
        prev_article_id -> Nullable<Integer>,
        next_article_id -> Nullable<Integer>,
        first_child_article_id -> Nullable<Integer>,
        title -> Text,
        poster -> Nullable<Text>,
        posted_at -> Timestamp,
        flags -> Nullable<Integer>,
        data_flavor -> Nullable<Text>,
        data -> Nullable<Text>,
    }
}
