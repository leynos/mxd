#[cfg(feature = "postgres")]
use temp_env::with_var;
#[cfg(feature = "postgres")]
use test_util::PostgresTestDb;

#[cfg(feature = "postgres")]
#[test]
fn external_postgres_is_used() -> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::var("POSTGRES_TEST_URL")
        .unwrap_or_else(|_| String::from("postgres://postgres:password@localhost/test"));
    let idx = base.rfind('/').expect("url has path");
    let prefix = &base[..=idx];
    with_var("POSTGRES_TEST_URL", Some(&base), || {
        let db = PostgresTestDb::new()?;
        assert!(!db.uses_embedded());
        assert!(db.url.starts_with(prefix));
        assert_ne!(db.url, base);
        Ok::<_, Box<dyn std::error::Error>>(())
    })
}
