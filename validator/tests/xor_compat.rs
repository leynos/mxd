//! Integration tests for XOR-encoded login fields via the `SynHX` client.
//!
//! The pinned `SynHX` client can still exercise XOR-obfuscated login payloads
//! against `mxd-wireframe-server`, even though its legacy news commands do not
//! match the server's newer news transaction surface.

use test_util::{AnyError, setup_login_db};
use validator::{
    ValidatorHarness,
    close_hx,
    connect_expect_timeout,
    expect_output_with_timeout,
    send_line_and_expect,
};

#[test]
fn xor_compat_validation() -> Result<(), AnyError> {
    let Some(harness) = ValidatorHarness::prepare()? else {
        return Ok(());
    };

    let server = harness.start_server_with_setup(|db| setup_login_db(db.into()))?;
    let bind_addr = server.bind_addr();
    let mut session = harness.spawn_hx()?;

    send_line_and_expect(
        &mut session,
        "/set encode 1",
        "encode = 1|adding variable encode",
        "hx did not confirm XOR encode toggle",
    )?;
    session.send_line(format!(
        "/server -l alice -p secret {} {}",
        bind_addr.ip(),
        bind_addr.port()
    ))?;
    expect_output_with_timeout(
        &mut session,
        "(?i)connected",
        "hx did not connect to the wireframe server with XOR enabled",
        connect_expect_timeout(),
    )?;

    close_hx(&mut session);
    Ok(())
}
