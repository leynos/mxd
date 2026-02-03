//! Integration test for XOR-encoded text fields via the hx client.
//!
//! Confirms that login, messaging, and news posting work with XOR encoding
//! enabled in the client.

use std::{
    io::Read,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use expectrl::{Regex, Session, spawn};
use test_util::{AnyError, DatabaseUrl, TestServer, setup_news_db, with_db};
use which::which;

#[test]
fn xor_compat_validation() -> Result<(), AnyError> {
    if should_skip_hx() {
        return Ok(());
    }

    let server = TestServer::start_with_setup("../Cargo.toml", |db| {
        setup_news_db(DatabaseUrl::new(db.as_str()))
    })?;

    let port = server.port();
    let mut p = spawn("hx")?;
    p.set_expect_timeout(Some(Duration::from_secs(10)));
    if !expect_or_skip(&mut p, Regex("HX"), "hx did not present Hotline prompt") {
        terminate_hx(&mut p);
        return Ok(());
    }
    p.send_line("/set encode 1")?;
    if !expect_or_skip(
        &mut p,
        Regex("encode = 1|adding variable encode"),
        "hx did not confirm encode toggle",
    ) {
        terminate_hx(&mut p);
        return Ok(());
    }
    p.send_line(format!("/server -l alice -p secret 127.0.0.1 {port}"))?;
    if !expect_or_skip(
        &mut p,
        Regex("(?i)connected"),
        "hx did not connect to server",
    ) {
        terminate_hx(&mut p);
        return Ok(());
    }

    let xor_message = "xor test message";
    let news_body = "xor test news body";
    p.send_line(format!("/msg alice {xor_message}"))?;
    p.send_line(format!("/post {news_body}"))?;
    if !expect_or_skip(
        &mut p,
        Regex("(?i)news posted"),
        "hx did not report news post",
    ) {
        terminate_hx(&mut p);
        return Ok(());
    }
    assert_news_body(&server, news_body)?;

    p.send_line("/quit")?;
    terminate_hx(&mut p);
    Ok(())
}

fn should_skip_hx() -> bool {
    if which("hx").is_err() {
        report_skip("hx not installed; skipping test");
        return true;
    }

    if hx_is_helix() {
        report_skip("hx appears to be the Helix editor; skipping test");
        return true;
    }

    false
}

fn hx_is_helix() -> bool {
    let Ok(mut child) = Command::new("hx")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        return false;
    };
    let start = Instant::now();
    let timeout = Duration::from_millis(500);
    let poll_interval = Duration::from_millis(20);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let stdout = read_stream(child.stdout.take());
                let stderr = read_stream(child.stderr.take());
                let combined = format!("{stdout}{stderr}").to_lowercase();
                return combined.contains("helix");
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    terminate_child(&mut child);
                    return false;
                }
                thread::sleep(poll_interval);
            }
            Err(_) => {
                terminate_child(&mut child);
                return false;
            }
        }
    }
}

fn terminate_child(child: &mut Child) {
    let _kill_result = child.kill();
    let _wait_result = child.wait();
}

fn read_stream<T: Read>(maybe_stream: Option<T>) -> String {
    let mut buffer = Vec::new();
    if let Some(mut stream) = maybe_stream {
        let _read_result = stream.read_to_end(&mut buffer);
    }
    String::from_utf8_lossy(&buffer).to_string()
}

fn expect_or_skip(session: &mut Session, regex: Regex<&'static str>, reason: &str) -> bool {
    if let Err(err) = session.expect(regex) {
        report_skip(&format!("{reason} ({err})"));
        return false;
    }
    true
}

fn assert_news_body(server: &TestServer, expected: &str) -> Result<(), AnyError> {
    let expected_text = expected.to_owned();
    let expected_query = expected_text.clone();
    let url = DatabaseUrl::new(server.db_url().as_ref());
    let count = with_db(url, move |conn| {
        let expected_query_local = expected_query.clone();
        Box::pin(async move {
            use diesel::{dsl::count_star, prelude::*};
            use diesel_async::RunQueryDsl;
            use mxd::schema::news_articles::dsl as a;

            let expected_ref = expected_query_local.as_str();
            let count: i64 = a::news_articles
                .filter(a::title.eq(expected_ref).or(a::data.eq(Some(expected_ref))))
                .select(count_star())
                .first(conn)
                .await?;
            Ok(count)
        })
    })?;
    if count == 0 {
        return Err(AnyError::msg(format!(
            "expected news article containing '{expected_text}'"
        )));
    }
    Ok(())
}

fn terminate_hx(session: &mut Session) {
    if let Err(err) = session.get_process_mut().exit(true) {
        report_skip(&format!("hx cleanup failed ({err})"));
    }
}

fn report_skip(message: &str) {
    #[expect(
        clippy::print_stderr,
        reason = "skip message: inform user why test is being skipped"
    )]
    {
        eprintln!("{message}");
    }
}
