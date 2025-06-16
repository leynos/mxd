#[cfg(feature = "postgres")]
use temp_env::with_var;
#[cfg(feature = "postgres")]
use test_util::PostgresTestDb;

#[cfg(feature = "postgres")]
#[test]
fn external_postgres_is_used() -> Result<(), Box<dyn std::error::Error>> {
    with_var("POSTGRES_TEST_URL", Some("postgres://example"), || {
        let db = PostgresTestDb::new()?;
        assert_eq!(db.url, "postgres://example");
        assert!(db.pg.is_none());
        Ok::<_, Box<dyn std::error::Error>>(())
    })
}
