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

use crate::AnyError;

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
        if is_listening(addr) {
            return Ok(());
        }
        if start.elapsed() >= STARTUP_TIMEOUT {
            warn!(?addr, "server did not open listening port before timeout");
            return Err(anyhow::anyhow!("server failed to open listening port"));
        }
        if let Some(status) = child.wait_timeout(POLL_INTERVAL)? {
            return Err(anyhow::anyhow!("server exited before readiness ({status})"));
        }
    }
}

fn is_listening(addr: SocketAddr) -> bool {
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok()
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
        time::Duration,
    };

    use rstest::{fixture, rstest};

    use super::wait_for_listening;
    use crate::AnyError;

    #[fixture]
    fn listening_socket() -> TcpListener {
        TcpListener::bind("127.0.0.1:0").expect("listen socket should bind")
    }

    #[fixture]
    fn unused_addr() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("ephemeral socket should bind");
        let addr = listener
            .local_addr()
            .expect("ephemeral socket should provide a local address");
        drop(listener);
        addr
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
    fn wait_for_listening_times_out(unused_addr: SocketAddr) {
        let result = wait_for_listening(unused_addr, Duration::from_millis(150));
        assert!(
            result.is_err(),
            "expected readiness to time out for closed port"
        );
    }
}
