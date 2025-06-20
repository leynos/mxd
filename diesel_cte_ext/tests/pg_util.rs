#[cfg(feature = "postgres")]
use diesel::pg::PgConnection;
#[cfg(feature = "postgres")]
use diesel_async::AsyncConnection;
#[cfg(feature = "postgres")]
use diesel_async::AsyncPgConnection;
#[cfg(feature = "postgres")]
use postgresql_embedded::PostgreSQL;

#[cfg(feature = "postgres")]
pub fn with_pg_sync<F, R>(f: F) -> R
where
    F: FnOnce(&mut PgConnection) -> R,
{
    let mut pg = PostgreSQL::default();
    pg.setup().unwrap();
    pg.start().unwrap();
    pg.create_database("test").unwrap();
    let url = pg.settings().url("test");
    let result = {
        let mut conn = PgConnection::establish(&url).unwrap();
        f(&mut conn)
    };
    pg.stop().unwrap();
    result
}

#[cfg(feature = "postgres")]
pub async fn with_pg_async<F, Fut, R>(f: F) -> R
where
    F: FnOnce(&mut AsyncPgConnection) -> Fut,
    Fut: std::future::Future<Output = R>,
{
    let mut pg = PostgreSQL::default();
    pg.setup().await.unwrap();
    pg.start().await.unwrap();
    pg.create_database("test").await.unwrap();
    let url = pg.settings().url("test");
    let result = {
        let mut conn = AsyncPgConnection::establish(&url).await.unwrap();
        f(&mut conn).await
    };
    pg.stop().await.unwrap();
    result
}
