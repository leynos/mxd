//! Spawns the server process on an ephemeral port and waits for readiness.

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener},
    process::Child,
};

use tracing::{debug, info, warn};

use super::{DbUrl, ServerTarget, build_server_command, wait_for_server};
use crate::AnyError;

const MAX_SERVER_LAUNCH_ATTEMPTS: u8 = 3;

/// Maps an unspecified bind address to the loopback address that reaches it.
const fn readiness_address(addr: SocketAddr) -> SocketAddr {
    match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
        }
        IpAddr::V6(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), addr.port())
        }
        _ => addr,
    }
}

/// Picks a free port on `bind_host` and releases it for the server to claim.
fn reserve_ephemeral_addr(bind_host: &str) -> Result<SocketAddr, AnyError> {
    let socket = TcpListener::bind((bind_host, 0))?;
    let addr = socket.local_addr()?;
    drop(socket);
    Ok(addr)
}

/// Kills and reaps a server that never became ready.
#[expect(
    clippy::let_underscore_must_use,
    reason = "best-effort cleanup; error already being propagated"
)]
fn abandon_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The result of one launch attempt that got as far as spawning the server.
enum LaunchOutcome {
    /// The server accepted a connection.
    Ready(Child, SocketAddr),
    /// The server never became ready, and has been killed and reaped.
    NotReady(AnyError),
}

/// Starts the server process listening on `addr`.
fn spawn_server(
    target: &ServerTarget,
    addr: SocketAddr,
    db_url: &DbUrl,
    attempt: u8,
) -> Result<Child, AnyError> {
    info!(
        port = addr.port(),
        db_url = %db_url,
        attempt,
        "launching server"
    );
    let child = build_server_command(target, addr, db_url).spawn()?;
    debug!("spawned server process, waiting for readiness");
    Ok(child)
}

/// Waits for `child` to accept connections, killing and reaping it on failure.
fn await_readiness(mut child: Child, addr: SocketAddr, attempt: u8) -> LaunchOutcome {
    match wait_for_server(&mut child, readiness_address(addr)) {
        Ok(()) => {
            info!(port = addr.port(), attempt, "server ready");
            LaunchOutcome::Ready(child, addr)
        }
        Err(error) => {
            warn!(error = %error, attempt, "wait_for_server failed");
            abandon_child(&mut child);
            LaunchOutcome::NotReady(error)
        }
    }
}

/// Spawns the server once and waits for its socket to accept connections.
///
/// A failure to reserve a port or spawn the process is returned as an error and
/// is not retried; a server that spawns but never becomes ready is reported as
/// [`LaunchOutcome::NotReady`] so the caller can retry.
fn launch_server_once(
    target: &ServerTarget,
    bind_host: &str,
    db_url: &DbUrl,
    attempt: u8,
) -> Result<LaunchOutcome, AnyError> {
    let addr = reserve_ephemeral_addr(bind_host)?;
    let child = spawn_server(target, addr, db_url, attempt)?;
    Ok(await_readiness(child, addr, attempt))
}

/// Spawns the configured server process on an ephemeral port and waits for the
/// socket to accept connections before returning the child handle and chosen
/// port.
pub(super) fn launch_server_process(
    target: &ServerTarget,
    bind_host: &str,
    db_url: &DbUrl,
) -> Result<(Child, SocketAddr), AnyError> {
    for attempt in 1..=MAX_SERVER_LAUNCH_ATTEMPTS {
        match launch_server_once(target, bind_host, db_url, attempt)? {
            LaunchOutcome::Ready(child, addr) => return Ok((child, addr)),
            LaunchOutcome::NotReady(error) if attempt == MAX_SERVER_LAUNCH_ATTEMPTS => {
                return Err(error);
            }
            LaunchOutcome::NotReady(_) => warn!(
                attempt,
                max_attempts = MAX_SERVER_LAUNCH_ATTEMPTS,
                "retrying server launch after readiness failure"
            ),
        }
    }
    Err(anyhow::anyhow!(
        "exhausted server launch attempts without readiness success"
    ))
}
