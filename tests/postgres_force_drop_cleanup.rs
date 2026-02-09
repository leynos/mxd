//! Integration tests for force-drop cleanup of embedded `PostgreSQL` test databases.

#[cfg(feature = "postgres")]
use anyhow::{Result, anyhow};
#[cfg(feature = "postgres")]
use diesel::{
    Connection,
    QueryableByName,
    RunQueryDsl,
    pg::PgConnection,
    sql_query,
    sql_types::{Bool, Text},
};
#[cfg(feature = "postgres")]
use test_util::{PostgresTestDb, postgres::PostgresTestDbError};
#[cfg(feature = "postgres")]
use url::Url;

#[cfg(feature = "postgres")]
#[derive(Debug, QueryableByName)]
struct DatabasePresence {
    #[diesel(sql_type = Bool)]
    present: bool,
}

#[cfg(feature = "postgres")]
fn derive_admin_url_and_db_name(database_url: &str) -> Result<(String, String)> {
    let mut parsed = Url::parse(database_url)?;
    let db_name = parsed.path().trim_start_matches('/').to_owned();
    if db_name.is_empty() {
        return Err(anyhow!(
            "database URL is missing a database name path segment"
        ));
    }
    parsed.set_path("/postgres");
    Ok((parsed.to_string(), db_name))
}

#[cfg(feature = "postgres")]
#[test]
fn dropping_fast_database_forcibly_terminates_leaked_connections() -> Result<()> {
    let db = match PostgresTestDb::new_from_template() {
        Ok(db) => db,
        Err(PostgresTestDbError::Unavailable(_)) => {
            tracing::warn!("skipping test: embedded PostgreSQL unavailable");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };

    let (admin_url, db_name) = derive_admin_url_and_db_name(db.url.as_ref())?;
    let mut leaked_connection = PgConnection::establish(db.url.as_ref())?;
    sql_query("SELECT 1").execute(&mut leaked_connection)?;

    drop(db);

    if sql_query("SELECT 1")
        .execute(&mut leaked_connection)
        .is_ok()
    {
        return Err(anyhow!(
            "force-drop should terminate leaked client sessions"
        ));
    }

    let mut admin_connection = PgConnection::establish(&admin_url)?;
    let presence =
        sql_query("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1) AS present")
            .bind::<Text, _>(&db_name)
            .get_result::<DatabasePresence>(&mut admin_connection)?;
    if presence.present {
        return Err(anyhow!(
            "force-drop should remove the cloned database after fixture teardown"
        ));
    }

    Ok(())
}
