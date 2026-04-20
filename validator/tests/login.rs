//! Integration tests for login validation via the `SynHX` client.
//!
//! These tests assert that the validator harness targets the prebuilt
//! `mxd-wireframe-server` binary explicitly and that login succeeds and fails
//! on observable client behaviour.

use test_util::{AnyError, setup_files_db, setup_login_db};
use validator::{
    ValidatorHarness,
    close_hx,
    connect_expect_timeout,
    expect_no_match,
    expect_output_with_timeout,
};

#[test]
fn login_validation_uses_wireframe_server_and_authenticates() -> Result<(), AnyError> {
    let Some(harness) = ValidatorHarness::prepare()? else {
        return Ok(());
    };

    let server = harness.start_server_with_setup(|db| setup_login_db(db.into()))?;
    let bind_addr = server.bind_addr();
    let mut session = harness.spawn_hx()?;

    session.send_line(format!(
        "/server -l alice -p secret {} {}",
        bind_addr.ip(),
        bind_addr.port()
    ))?;
    expect_output_with_timeout(
        &mut session,
        "(?i)connected",
        "hx did not connect to the wireframe server",
        connect_expect_timeout(),
    )?;

    close_hx(&mut session);
    Ok(())
}

#[test]
fn login_validation_rejects_invalid_credentials() -> Result<(), AnyError> {
    let Some(harness) = ValidatorHarness::prepare()? else {
        return Ok(());
    };

    let server = harness.start_server_with_setup(|db| setup_files_db(db.into()))?;
    let bind_addr = server.bind_addr();
    let mut session = harness.spawn_hx()?;

    session.send_line(format!(
        "/server -l alice -p wrong-password {} {}",
        bind_addr.ip(),
        bind_addr.port()
    ))?;
    expect_output_with_timeout(
        &mut session,
        "(?i)login failed\\?",
        "hx did not report failed authentication",
        connect_expect_timeout(),
    )?;
    session.send_line("/ls")?;
    expect_no_match(
        &mut session,
        "fileA\\.txt",
        "hx reached an authenticated file listing after invalid credentials",
    )?;

    close_hx(&mut session);
    Ok(())
}
