//! Readiness checks for spawned servers.

#[cfg(test)]
use std::thread;
use std::{
    net::{SocketAddr, TcpStream},
    process::Child,
    time::{Duration, Instant},
};

use tracing::warn;
use wait_timeout::ChildExt;

use crate::{AnyError, protocol::handshake};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Wait for a spawned server to accept connections on the provided address.
///
/// # Errors
///
/// Returns an error if the server exits early or fails to start listening
/// before the startup timeout elapses.
pub(super) fn wait_for_server(child: &mut Child, addr: SocketAddr) -> Result<(), AnyError> {
    let start = Instant::now();
    loop {
        check_child_alive(child)?;
        if is_protocol_ready(addr) {
            return verify_ready_server(child);
        }
        check_timeout(&start, addr)?;
        wait_or_fail_if_exited(child)?;
    }
}

fn check_child_alive(child: &mut Child) -> Result<(), AnyError> {
    if let Some(status) = child.wait_timeout(Duration::ZERO)? {
        return Err(anyhow::anyhow!("server exited before readiness ({status})"));
    }
    Ok(())
}

fn verify_ready_server(child: &mut Child) -> Result<(), AnyError> {
    check_child_alive(child)?;
    Ok(())
}

fn check_timeout(start: &Instant, addr: SocketAddr) -> Result<(), AnyError> {
    if start.elapsed() >= STARTUP_TIMEOUT {
        warn!(?addr, "server did not open listening port before timeout");
        return Err(anyhow::anyhow!("server failed to open listening port"));
    }
    Ok(())
}

fn wait_or_fail_if_exited(child: &mut Child) -> Result<(), AnyError> {
    if let Some(status) = child.wait_timeout(POLL_INTERVAL)? {
        return Err(anyhow::anyhow!("server exited before readiness ({status})"));
    }
    Ok(())
}

#[cfg(test)]
fn is_listening(addr: SocketAddr) -> bool {
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok()
}

fn is_protocol_ready(addr: SocketAddr) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) else {
        return false;
    };
    if stream.set_read_timeout(Some(CONNECT_TIMEOUT)).is_err()
        || stream.set_write_timeout(Some(CONNECT_TIMEOUT)).is_err()
    {
        return false;
    }
    handshake(&mut stream).is_ok()
}

#[cfg(test)]
fn wait_for_listening(addr: SocketAddr, timeout: Duration) -> Result<(), AnyError> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if is_listening(addr) {
            return Ok(());
        }
        thread::sleep(POLL_INTERVAL);
    }
    Err(anyhow::anyhow!("port did not become ready"))
}

#[cfg(test)]
mod tests {
    use std::{
        net::{SocketAddr, TcpListener},
        process::Command,
        time::Duration,
    };

    use rstest::{fixture, rstest};

    use super::{wait_for_listening, wait_for_server};
    use crate::AnyError;

    #[fixture]
    fn listening_socket() -> TcpListener {
        TcpListener::bind("localhost:0").expect("listen socket should bind")
    }

    #[rstest]
    fn wait_for_listening_reports_ready(listening_socket: TcpListener) -> Result<(), AnyError> {
        let addr = listening_socket
            .local_addr()
            .expect("listening socket should provide a local address");
        wait_for_listening(addr, Duration::from_millis(200))?;
        Ok(())
    }

    #[rstest]
    fn wait_for_listening_times_out() {
        let closed_addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let result = wait_for_listening(closed_addr, Duration::from_millis(150));
        assert!(
            result.is_err(),
            "expected readiness to time out for closed port"
        );
    }

    #[rstest]
    fn wait_for_server_rejects_exited_child_even_if_port_is_open(listening_socket: TcpListener) {
        let addr = listening_socket
            .local_addr()
            .expect("listening socket should provide a local address");
        let mut child = Command::new("rustc")
            .arg("--version")
            .spawn()
            .expect("rustc should spawn");
        child.wait().expect("rustc should exit");

        let result = wait_for_server(&mut child, addr);

        assert!(
            result.is_err(),
            "readiness must fail if the spawned child already exited"
        );
    }
}
