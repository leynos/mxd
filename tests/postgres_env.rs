#[cfg(feature = "postgres")]
use temp_env::with_var;
#[cfg(feature = "postgres")]
use test_util::PostgresTestDb;

#[cfg(feature = "postgres")]
#[test]
fn external_postgres_is_used() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
            }
            Err(e) => {
                if e.downcast_ref::<test_util::postgres::PostgresUnavailable>()
                    .is_some()
                {
                    eprintln!("skipping test: {e}");
                    return Ok(());
                }
                Err(e)
            }
        },
    )
}
