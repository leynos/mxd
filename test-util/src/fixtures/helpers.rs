//! Private helpers for fixture setup.

use mxd::{
    db::{DbConnection, create_bundle},
    models::{NewArticle, NewBundle},
};

use crate::AnyError;

pub(super) async fn insert_root_bundle(conn: &mut DbConnection) -> Result<i32, AnyError> {
    let id = create_bundle(
        conn,
        &NewBundle {
            parent_bundle_id: None,
            name: "Bundle",
        },
    )
    .await?;

    Ok(id)
}

pub(super) async fn insert_article(
    conn: &mut DbConnection,
    article: &NewArticle<'_>,
) -> Result<i32, AnyError> {
    use diesel_async::RunQueryDsl;
    use mxd::schema::news_articles::dsl as a;

    #[cfg(feature = "postgres")]
    let inserted_id: i32 = diesel::insert_into(a::news_articles)
        .values(article)
        .returning(a::id)
        .get_result(conn)
        .await?;

    #[cfg(not(feature = "postgres"))]
    let inserted_id: i32 = {
        use diesel::sql_types::Integer;
        diesel::insert_into(a::news_articles)
            .values(article)
            .execute(conn)
            .await?;
        diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()"))
            .get_result(conn)
            .await?
    };

    Ok(inserted_id)
}
