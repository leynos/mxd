#[cfg(feature = "postgres")]
use test_util::setup_postgres_for_test;

#[cfg(feature = "postgres")]
#[test]
/// Tests that an external Postgres URL from the `POSTGRES_TEST_URL` environment variable is used instead of creating an internal Postgres instance.
///
/// This test sets the `POSTGRES_TEST_URL` environment variable, invokes `setup_postgres_for_test`, and verifies that the returned URL matches the environment variable and that no internal Postgres instance is created. The environment variable is cleaned up after the test.
///
/// # Examples
///
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
