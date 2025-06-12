#[cfg(feature = "postgres")]
use test_util::setup_postgres_for_test;

#[cfg(feature = "postgres")]
#[test]
    std::env::set_var("POSTGRES_TEST_URL", "postgres://example");
    std::env::remove_var("POSTGRES_TEST_URL");
/// ```
/// // This test is intended to be run with the "postgres" feature enabled.
/// external_postgres_is_used().unwrap();
/// ```
fn external_postgres_is_used() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        std::env::set_var("POSTGRES_TEST_URL", "postgres://example");
    }
    let (url, pg) = setup_postgres_for_test(|_| Ok(()))?;
    assert_eq!(url, "postgres://example");
    assert!(pg.is_none());
    unsafe {
        std::env::remove_var("POSTGRES_TEST_URL");
    }
    Ok(())
}
