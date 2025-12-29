//! Custom connection handler for Hotline protocol framing.
//!
//! This module provides a connection handler that uses [`HotlineCodec`] for frame
//! decoding/encoding, bypassing the wireframe library's built-in length-delimited
//! codec. This is necessary because Hotline uses 20-byte header framing, which is
//! incompatible with wireframe's default framing format.
//!
//! # Architecture
//!
//! The connection handler:
//!
//! 1. Receives a TCP stream after preamble handling completes
//! 2. Wraps it with [`HotlineCodec`] via [`Framed`]
//! 3. Processes each decoded [`HotlineTransaction`] through the routing logic
//! 4. Sends reply transactions back to the client

use std::{io, net::SocketAddr, sync::Arc};

use futures_util::{SinkExt, StreamExt};
use tokio::{net::TcpStream, sync::Mutex as TokioMutex};
use tokio_util::codec::Framed;
use tracing::{error, info, warn};
use wireframe::rewind_stream::RewindStream;

use super::{
    codec::{HotlineCodec, HotlineTransaction},
    routes::process_transaction_bytes,
};
use crate::{db::DbPool, handler::Session};

/// Handle a connection after the preamble handshake completes.
///
/// This function processes Hotline transactions using the [`HotlineCodec`] for
/// proper 20-byte header framing, then routes them through the domain command
/// dispatcher.
///
/// # Arguments
///
/// * `stream` - A [`RewindStream`] wrapping the TCP connection with any leftover bytes from
///   preamble parsing.
/// * `peer` - The remote peer's socket address.
/// * `pool` - Database connection pool for command handlers.
/// * `session` - Per-connection session state wrapped in a mutex.
///
/// # Errors
///
/// Returns an I/O error if the connection encounters a transport-level failure.
#[expect(
    clippy::cognitive_complexity,
    reason = "connection loop with match arms has inherent complexity"
)]
pub async fn handle_connection(
    stream: RewindStream<TcpStream>,
    peer: SocketAddr,
    pool: DbPool,
    session: Arc<TokioMutex<Session>>,
) -> io::Result<()> {
    info!(peer = %peer, "handling hotline connection");

    let mut framed = Framed::new(stream, HotlineCodec::new());

    loop {
        match framed.next().await {
            Some(Ok(tx)) => {
                let reply = process_transaction(tx, peer, pool.clone(), &session).await;
                if let Err(e) = framed.send(reply).await {
                    error!(peer = %peer, error = %e, "failed to send reply");
                    return Err(e);
                }
            }
            Some(Err(e)) => {
                warn!(peer = %peer, error = %e, "frame decode error");
                // Continue processing; the client may send valid frames later
            }
            None => {
                info!(peer = %peer, "connection closed by peer");
                break;
            }
        }
    }

    Ok(())
}

/// Process a single Hotline transaction and return the reply.
#[expect(
    clippy::cognitive_complexity,
    reason = "sequential error handling has inherent complexity"
)]
async fn process_transaction(
    tx: HotlineTransaction,
    peer: SocketAddr,
    pool: DbPool,
    session: &Arc<TokioMutex<Session>>,
) -> HotlineTransaction {
    // Convert to raw bytes for compatibility with existing routing logic
    let frame_bytes = hotline_transaction_to_bytes(&tx);

    // Process through the domain command dispatcher
    let reply_bytes = {
        let mut session_guard = session.lock().await;
        process_transaction_bytes(&frame_bytes, peer, pool, &mut session_guard).await
    };

    // Parse reply bytes back into a HotlineTransaction
    match bytes_to_hotline_transaction(&reply_bytes) {
        Ok(reply) => reply,
        Err(e) => {
            error!(error = %e, "failed to parse reply transaction");
            // Return an error transaction
            error_transaction(tx.header())
        }
    }
}

/// Convert a [`HotlineTransaction`] to raw bytes.
fn hotline_transaction_to_bytes(tx: &HotlineTransaction) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(crate::transaction::HEADER_LEN + tx.payload().len());
    let mut header_buf = [0u8; crate::transaction::HEADER_LEN];
    tx.header().write_bytes(&mut header_buf);
    bytes.extend_from_slice(&header_buf);
    bytes.extend_from_slice(tx.payload());
    bytes
}

/// Parse raw bytes into a [`HotlineTransaction`].
#[expect(
    clippy::expect_used,
    reason = "slice length guaranteed by preceding get()"
)]
fn bytes_to_hotline_transaction(bytes: &[u8]) -> io::Result<HotlineTransaction> {
    let header_bytes = bytes
        .get(..crate::transaction::HEADER_LEN)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "reply too short for header"))?;
    let header = crate::transaction::FrameHeader::from_bytes(
        header_bytes
            .try_into()
            .expect("slice length guaranteed by get()"),
    );
    let payload = bytes
        .get(crate::transaction::HEADER_LEN..)
        .unwrap_or(&[])
        .to_vec();
    Ok(HotlineTransaction::from_parts(header, payload))
}

/// Build an error transaction as a reply.
const fn error_transaction(req_header: &crate::transaction::FrameHeader) -> HotlineTransaction {
    const ERR_INTERNAL: u32 = 3;

    let header = crate::transaction::FrameHeader {
        flags: 0,
        is_reply: 1,
        ty: req_header.ty,
        id: req_header.id,
        error: ERR_INTERNAL,
        total_size: 0,
        data_size: 0,
    };
    HotlineTransaction::from_parts(header, Vec::new())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::transaction::FrameHeader;

    #[rstest]
    fn roundtrip_transaction_bytes() {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 42,
            error: 0,
            total_size: 5,
            data_size: 5,
        };
        let tx = HotlineTransaction::from_parts(header.clone(), b"hello".to_vec());

        let bytes = hotline_transaction_to_bytes(&tx);
        let parsed = bytes_to_hotline_transaction(&bytes).expect("parse should succeed");

        assert_eq!(parsed.header().ty, header.ty);
        assert_eq!(parsed.header().id, header.id);
        assert_eq!(parsed.payload(), b"hello");
    }

    #[rstest]
    fn error_transaction_sets_reply_flag() {
        let req_header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 200,
            id: 99,
            error: 0,
            total_size: 0,
            data_size: 0,
        };

        let err_tx = error_transaction(&req_header);

        assert_eq!(err_tx.header().is_reply, 1);
        assert_eq!(err_tx.header().ty, 200);
        assert_eq!(err_tx.header().id, 99);
        assert_eq!(err_tx.header().error, 3);
    }
}
