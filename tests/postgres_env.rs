#[cfg(feature = "postgres")]
use postgresql_embedded::PostgreSQL;
use test_util::TestServer;

#[cfg(feature = "postgres")]
#[test]
fn reuse_external_postgres_via_env() -> Result<(), Box<dyn std::error::Error>> {
    let mut pg = PostgreSQL::default();
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        pg.setup().await?;
        pg.start().await?;
        pg.create_database("test_env").await?;
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    let url = pg.settings().url("test_env");
    std::env::set_var("POSTGRES_TEST_URL", &url);
    let server = TestServer::start("./Cargo.toml")?;
    assert_eq!(server.db_url(), url);
    assert!(!server.uses_embedded_postgres());
    std::env::remove_var("POSTGRES_TEST_URL");
    drop(server);
    rt.block_on(pg.stop())?;
    Ok(())
}
