//! Integration tests verifying external `PostgreSQL` test database configuration.
#[cfg(feature = "postgres")]
use test_util::{AnyError, PostgresTestDb, postgres::PostgresTestDbError, with_env_var};

/// Validates that an opened database is an external (non-embedded) instance
/// and that its URL has been assigned a unique suffix by the test harness.
#[cfg(feature = "postgres")]
fn validate_external_db(db: &PostgresTestDb, base: &str, prefix: &str) -> Result<(), AnyError> {
    if db.uses_embedded() {
        return Err(anyhow::anyhow!("expected external PostgreSQL instance"));
    }
    if !db.url.starts_with(prefix) {
        return Err(anyhow::anyhow!("expected URL with prefix {prefix}"));
    }
    if db.url.as_ref() == base {
        return Err(anyhow::anyhow!("expected database URL to be updated"));
    }
    Ok(())
}

/// Handles the case where `PostgreSQL` is reported as unreachable.
///
/// If the caller had an explicit `POSTGRES_TEST_URL` configured, the
/// unreachable state is a hard failure. Otherwise the test is skipped with a
/// warning.
#[cfg(feature = "postgres")]
fn handle_unavailable_postgres(configured_external_url: Option<&str>) -> Result<(), AnyError> {
    if configured_external_url.is_some() {
        return Err(anyhow::anyhow!(
            "expected configured external PostgreSQL instance to be reachable"
        ));
    }
    tracing::warn!("skipping test: PostgreSQL unavailable");
    Ok(())
}

#[cfg(feature = "postgres")]
#[test]
fn external_postgres_is_used() -> Result<(), AnyError> {
    let configured_external_url = std::env::var("POSTGRES_TEST_URL").ok();
    let base = configured_external_url
        .clone()
        .unwrap_or_else(|| "postgres://postgres:password@localhost/test".to_owned());
    let idx = base
        .rfind('/')
        .ok_or_else(|| anyhow::anyhow!("POSTGRES_TEST_URL missing path"))?;
    let prefix = base
        .get(..=idx)
        .ok_or_else(|| anyhow::anyhow!("POSTGRES_TEST_URL prefix invalid"))?;
    with_env_var(
        "POSTGRES_TEST_URL",
        Some(&base),
        || match PostgresTestDb::new() {
            Ok(db) => validate_external_db(&db, &base, prefix),
            Err(PostgresTestDbError::Unavailable(_)) => {
                handle_unavailable_postgres(configured_external_url.as_deref())
            }
            Err(e) => Err(e.into()),
        },
    )
}
