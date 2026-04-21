//! Integration tests verifying external `PostgreSQL` test database configuration.
#[cfg(feature = "postgres")]
use test_util::{AnyError, PostgresTestDb, postgres::PostgresTestDbError, with_env_var};

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
            Ok(db) => {
                if db.uses_embedded() {
                    return Err(anyhow::anyhow!("expected external PostgreSQL instance"));
                }
                if !db.url.starts_with(prefix) {
                    return Err(anyhow::anyhow!("expected URL with prefix {prefix}"));
                }
                if db.url.as_ref() == base {
                    return Err(anyhow::anyhow!("expected database URL to be updated"));
                }
                Ok::<_, AnyError>(())
            }
            Err(PostgresTestDbError::Unavailable(_)) => {
                if configured_external_url.is_some() {
                    return Err(anyhow::anyhow!(
                        "expected configured external PostgreSQL instance to be reachable"
                    ));
                }
                tracing::warn!("skipping test: PostgreSQL unavailable");
                Ok(())
            }
            Err(e) => Err(e.into()),
        },
    )
}
