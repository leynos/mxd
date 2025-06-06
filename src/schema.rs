diesel::table! {
    users (id) {
        id -> Integer,
        username -> Text,
        password -> Text,
    }
}
diesel::table! {
    news_categories (id) {
        id -> Integer,
        name -> Text,
    }
}
