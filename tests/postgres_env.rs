#![expect(missing_docs, reason = "test file")]
#[cfg(feature = "postgres")]
use temp_env::with_var;
#[cfg(feature = "postgres")]
use test_util::{AnyError, PostgresTestDb, postgres::PostgresTestDbError};

#[cfg(feature = "postgres")]
#[test]
fn external_postgres_is_used() -> Result<(), AnyError> {
    let base = std::env::var("POSTGRES_TEST_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost/test".to_owned());
    let idx = base
        .rfind('/')
        .ok_or_else(|| anyhow::anyhow!("POSTGRES_TEST_URL missing path"))?;
    let prefix = base
        .get(..=idx)
        .ok_or_else(|| anyhow::anyhow!("POSTGRES_TEST_URL prefix invalid"))?;
    with_var(
        "POSTGRES_TEST_URL",
        Some(&base),
        || match PostgresTestDb::new() {
            Ok(db) => {
                if db.uses_embedded() {
                    return Err(anyhow::anyhow!("expected external PostgreSQL instance"));
                }
                if !db.url.starts_with(prefix) {
                    return Err(anyhow::anyhow!("expected url with prefix {prefix}"));
                }
                if db.url.as_ref() == base {
                    return Err(anyhow::anyhow!("expected database url to be updated"));
                }
                Ok::<_, AnyError>(())
            }
            Err(PostgresTestDbError::Unavailable(_)) => {
                tracing::warn!("skipping test: PostgreSQL unavailable");
                Ok(())
            }
            Err(e) => Err(e.into()),
        },
    )
}
