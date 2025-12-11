#![allow(
    unfulfilled_lint_expectations,
    reason = "test lint expectations may not all trigger"
)]
#![expect(missing_docs, reason = "test file")]
#![expect(clippy::print_stderr, reason = "test diagnostics")]
#![expect(clippy::panic_in_result_fn, reason = "test assertions")]
#![expect(clippy::string_slice, reason = "test string manipulation")]

#[cfg(feature = "postgres")]
use temp_env::with_var;
#[cfg(feature = "postgres")]
use test_util::{AnyError, PostgresTestDb, postgres::PostgresTestDbError};

#[cfg(feature = "postgres")]
#[test]
fn external_postgres_is_used() -> Result<(), AnyError> {
    let base = std::env::var("POSTGRES_TEST_URL")
        .unwrap_or_else(|_| String::from("postgres://postgres:password@localhost/test"));
    let idx = base.rfind('/').expect("url has path");
    let prefix = &base[..=idx];
    with_var(
        "POSTGRES_TEST_URL",
        Some(&base),
        || match PostgresTestDb::new() {
            Ok(db) => {
                assert!(!db.uses_embedded());
                assert!(db.url.starts_with(prefix));
                assert_ne!(db.url.as_ref(), base);
                Ok::<_, AnyError>(())
            }
            Err(PostgresTestDbError::Unavailable(_)) => {
                eprintln!("skipping test: PostgreSQL unavailable");
                Ok(())
            }
            Err(e) => Err(Box::new(e) as AnyError),
        },
    )
}
