//! Integration tests for the concurrency and time-zone behaviour of the
//! embedded `PostgreSQL` harness.
//!
//! Both cases guard properties that the serial nextest profile used to provide
//! by accident, or that a change of `PostgreSQL` bundle could quietly break:
//!
//! - two test databases alive at once must be separate databases, because the parallel profile runs
//!   many fixtures concurrently; and
//! - the cluster must resolve `UTC`, because diesel's `PgConnection::establish` sends `SET TIME
//!   ZONE 'UTC'` before it returns a connection, so a bundle with incomplete time-zone data fails
//!   every database test at the first connection rather than in an assertion.

#[cfg(feature = "postgres")]
use anyhow::{Context, Result};
#[cfg(feature = "postgres")]
use diesel::{
    Connection,
    RunQueryDsl,
    pg::PgConnection,
    sql_query,
    sql_types::{Bool, Text},
};
#[cfg(feature = "postgres")]
use test_util::PostgresTestDb;

/// Reads a single text column from a one-row query.
#[cfg(feature = "postgres")]
fn query_text(connection: &mut PgConnection, sql: &str) -> Result<String> {
    #[derive(Debug, diesel::QueryableByName)]
    struct TextValue {
        #[diesel(sql_type = Text)]
        value: String,
    }

    let row = sql_query(sql)
        .get_result::<TextValue>(connection)
        .with_context(|| format!("running query {sql}"))?;
    Ok(row.value)
}

/// Reads a single boolean column from a one-row query.
#[cfg(feature = "postgres")]
fn query_bool(connection: &mut PgConnection, sql: &str) -> Result<bool> {
    #[derive(Debug, diesel::QueryableByName)]
    struct BoolValue {
        #[diesel(sql_type = Bool)]
        value: bool,
    }

    let row = sql_query(sql)
        .get_result::<BoolValue>(connection)
        .with_context(|| format!("running query {sql}"))?;
    Ok(row.value)
}

/// Two fixtures that are alive at the same time must hold separate databases.
///
/// Before the parallel profile this was guaranteed by running one test at a
/// time. The harness now runs many at once, so the isolation has to come from
/// the database names rather than from the schedule.
#[cfg(feature = "postgres")]
#[test]
fn concurrent_test_databases_are_isolated() -> Result<()> {
    let first = PostgresTestDb::new()?;
    let second = PostgresTestDb::new()?;

    let mut first_connection = PgConnection::establish(first.url.as_ref())?;
    let mut second_connection = PgConnection::establish(second.url.as_ref())?;

    let first_name = query_text(&mut first_connection, "SELECT current_database() AS value")?;
    let second_name = query_text(&mut second_connection, "SELECT current_database() AS value")?;
    anyhow::ensure!(
        first_name != second_name,
        "two live fixtures shared the database {first_name}"
    );

    // A table created through one fixture must be invisible to the other,
    // which is the property a later test actually depends on.
    sql_query("CREATE TABLE isolation_probe (id integer PRIMARY KEY)")
        .execute(&mut first_connection)
        .context("creating the probe table in the first database")?;
    let probe_visible = query_bool(
        &mut second_connection,
        "SELECT to_regclass('public.isolation_probe') IS NOT NULL AS value",
    )?;
    anyhow::ensure!(
        !probe_visible,
        "the probe table leaked into the second database"
    );

    Ok(())
}

/// The cluster must accept an explicit `UTC` time zone.
///
/// This is the regression test for a `PostgreSQL` bundle whose time-zone aliases
/// were not extracted: the zone data is missing, so `SET TimeZone` fails, and
/// diesel's connect handshake fails with it before any test assertion runs.
#[cfg(feature = "postgres")]
#[test]
fn cluster_accepts_the_utc_time_zone() -> Result<()> {
    let db = PostgresTestDb::new()?;
    let mut connection = PgConnection::establish(db.url.as_ref())?;

    // `SHOW` names its column after the variable, so alias it rather than
    // expecting the `AS value` the helper's other queries use.
    sql_query("SET TimeZone = 'UTC'").execute(&mut connection)?;
    let zone = query_text(
        &mut connection,
        "SELECT current_setting('TimeZone') AS value",
    )?;
    anyhow::ensure!(zone == "UTC", "the cluster resolved UTC as {zone}");

    // The localised alias must resolve too. It is a separate hard link in the
    // bundles that ship their own time-zone data, so a partially extracted
    // tree can accept UTC while failing Etc/UTC.
    sql_query("SET TimeZone = 'Etc/UTC'").execute(&mut connection)?;
    let etc_zone = query_text(
        &mut connection,
        "SELECT current_setting('TimeZone') AS value",
    )?;
    anyhow::ensure!(
        etc_zone == "Etc/UTC" || etc_zone == "UTC",
        "the cluster resolved Etc/UTC as {etc_zone}"
    );

    Ok(())
}
