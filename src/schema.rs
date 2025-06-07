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
