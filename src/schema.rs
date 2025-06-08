diesel::table! {
    users (id) {
        id -> Integer,
        username -> Text,
        password -> Text,
    }
}
diesel::table! {
    news_bundles (id) {
        id -> Integer,
        parent_bundle_id -> Nullable<Integer>,
        name -> Text,
    }
}
diesel::table! {
    news_categories (id) {
        id -> Integer,
        name -> Text,
        bundle_id -> Nullable<Integer>,
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
        flags -> Integer,
        data_flavor -> Nullable<Text>,
        data -> Nullable<Text>,
    }
}
