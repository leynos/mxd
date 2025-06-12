#[cfg(feature = "postgres")]
use test_util::setup_postgres_for_test;

#[cfg(feature = "postgres")]
#[test]
    set_var("POSTGRES_TEST_URL", "postgres://example");
    remove_var("POSTGRES_TEST_URL");

#[cfg(feature = "postgres")]
fn set_var(key: &str, value: &str) {
    unsafe { std::env::set_var(key, value) }
}

#[cfg(feature = "postgres")]
fn remove_var(key: &str) {
    unsafe { std::env::remove_var(key) }
}
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
