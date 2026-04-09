//! Wireframe handshake hooks for Hotline connections.
//!
//! This module wires the Hotline handshake semantics into the Wireframe runtime
//! by registering preamble callbacks that emit the standard 8-byte reply and
//! enforce the protocol's idle timeout, with reusable hooks for tests.

use std::{io, time::Duration};

use bincode::error::DecodeError;
use futures_util::{FutureExt, future::BoxFuture};
use tokio::net::TcpStream;
use tracing::warn;
use wireframe::{
    app::Packet,
    codec::FrameCodec,
    serializer::Serializer,
    server::{AppFactory, ServerState, WireframeServer},
};

use super::preamble::HotlinePreamble;
use crate::{
    protocol::{
        HANDSHAKE_ERR_INVALID,
        HANDSHAKE_ERR_TIMEOUT,
        HANDSHAKE_ERR_UNSUPPORTED_VERSION,
        HANDSHAKE_INVALID_PROTOCOL_TOKEN,
        HANDSHAKE_OK,
        HANDSHAKE_UNSUPPORTED_VERSION_TOKEN,
        write_handshake_reply,
    },
    wireframe::connection::{
        ConnectionContext,
        HandshakeMetadata,
        scope_current_context,
        store_current_context,
    },
};

/// Attach Hotline handshake behaviour to a [`WireframeServer`].
///
/// The returned server writes the Hotline reply on success, returns Hotline
/// error codes on decode failures, and times out idle sockets after `timeout`.
/// Tests may call this with a shorter duration, while production code should
/// use [`crate::protocol::HANDSHAKE_TIMEOUT`].
#[must_use]
pub fn install<F, S, Ser, Ctx, E, Codec>(
    server: WireframeServer<F, HotlinePreamble, S, Ser, Ctx, E, Codec>,
    timeout: Duration,
) -> WireframeServer<F, HotlinePreamble, S, Ser, Ctx, E, Codec>
where
    F: AppFactory<Ser, Ctx, E, Codec>,
    S: ServerState,
    Ser: Serializer + Send + Sync,
    Ctx: Send + 'static,
    E: Packet,
    Codec: FrameCodec,
{
    server
        .on_preamble_decode_success(success_handler())
        .on_preamble_decode_failure(failure_handler())
        .preamble_timeout(timeout)
}

fn success_handler()
-> impl for<'a> Fn(&'a HotlinePreamble, &'a mut TcpStream) -> BoxFuture<'a, io::Result<()>> + Send + Sync
{
    move |preamble, stream| {
        let mut context = ConnectionContext::new(HandshakeMetadata::from(preamble.handshake()));
        match stream.peer_addr() {
            Ok(peer) => {
                context = context.with_peer(peer);
            }
            Err(error) => {
                warn!(%error, "failed to retrieve peer address during handshake");
                return async move { Err(error) }.boxed();
            }
        }

        scope_current_context(Some(context.clone()), async move {
            store_current_context(context);
            write_handshake_reply(stream, HANDSHAKE_OK).await?;
            Ok(())
        })
        .boxed()
    }
}

fn failure_handler()
-> impl for<'a> Fn(&'a DecodeError, &'a mut TcpStream) -> BoxFuture<'a, io::Result<()>> + Send + Sync
{
    move |err, stream| {
        async move {
            if let Some(code) = error_code_for_decode(err) {
                write_handshake_reply(stream, code).await?;
            }
            Ok(())
        }
        .boxed()
    }
}

fn error_code_for_decode(err: &DecodeError) -> Option<u32> {
    match err {
        DecodeError::OtherString(text) => error_code_from_str(text),
        DecodeError::Other(text) => error_code_from_str(text),
        DecodeError::Io { inner, .. } if inner.kind() == io::ErrorKind::TimedOut => {
            Some(HANDSHAKE_ERR_TIMEOUT)
        }
        _ => None,
    }
}

fn is_invalid_protocol(text: &str) -> bool { text.starts_with(HANDSHAKE_INVALID_PROTOCOL_TOKEN) }

fn is_unsupported_version(text: &str) -> bool {
    text.starts_with(HANDSHAKE_UNSUPPORTED_VERSION_TOKEN)
}

fn error_code_from_str(text: &str) -> Option<u32> {
    if is_invalid_protocol(text) {
        Some(HANDSHAKE_ERR_INVALID)
    } else if is_unsupported_version(text) {
        Some(HANDSHAKE_ERR_UNSUPPORTED_VERSION)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rstest::rstest;
    use tokio::{io::AsyncWriteExt, net::TcpStream, sync::oneshot, time::timeout};
    use wireframe::{
        app::{Envelope, WireframeApp},
        serializer::BincodeSerializer,
        server::WireframeServer,
    };

    use super::HotlinePreamble;
    use crate::{
        protocol::{
            HANDSHAKE_ERR_INVALID,
            HANDSHAKE_ERR_TIMEOUT,
            HANDSHAKE_ERR_UNSUPPORTED_VERSION,
            HANDSHAKE_OK,
            HANDSHAKE_TIMEOUT,
            PROTOCOL_ID,
            VERSION,
        },
        wireframe::{
            connection::take_current_context,
            test_helpers::{preamble_bytes, recv_reply},
        },
    };

    pub(super) fn start_server(timeout: Duration) -> (std::net::SocketAddr, oneshot::Sender<()>) {
        let server = WireframeServer::new(|| {
            let handshake = take_current_context()
                .map(|context| context.into_parts().0)
                .unwrap_or_default();
            WireframeApp::<BincodeSerializer, (), Envelope>::default().app_data(handshake)
        })
        .with_preamble::<HotlinePreamble>();
        let server = super::install(server, timeout);
        let server = server
            .bind("127.0.0.1:0".parse().expect("parse socket addr"))
            .expect("bind");
        let addr = server.local_addr().expect("addr");
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = server
                .run_with_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await;
        });
        (addr, shutdown_tx)
    }

    #[rstest]
    #[tokio::test]
    async fn replies_success() {
        let (addr, shutdown) = start_server(HANDSHAKE_TIMEOUT);
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        let bytes = preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION, 7);
        stream.write_all(&bytes).await.expect("write handshake");

        let reply = recv_reply(&mut stream).await.expect("handshake reply");
        assert_eq!(&reply[0..4], PROTOCOL_ID);
        assert_eq!(
            u32::from_be_bytes(
                reply[4..8]
                    .try_into()
                    .expect("convert reply slice to array (ok)")
            ),
            HANDSHAKE_OK
        );
        let _ = shutdown.send(());
    }

    #[rstest]
    #[case(*b"WRNG", HANDSHAKE_ERR_INVALID)]
    #[case(*PROTOCOL_ID, HANDSHAKE_ERR_UNSUPPORTED_VERSION)]
    #[tokio::test]
    async fn replies_handshake_errors(#[case] protocol: [u8; 4], #[case] expected: u32) {
        let (addr, shutdown) = start_server(HANDSHAKE_TIMEOUT);
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        let version = if expected == HANDSHAKE_ERR_UNSUPPORTED_VERSION {
            VERSION + 1
        } else {
            VERSION
        };
        let bytes = preamble_bytes(protocol, *b"CHAT", version, 0);
        stream.write_all(&bytes).await.expect("write handshake");

        let reply = recv_reply(&mut stream).await.expect("handshake reply");
        assert_eq!(
            u32::from_be_bytes(
                reply[4..8]
                    .try_into()
                    .expect("convert reply slice to array (error path)")
            ),
            expected
        );
        let _ = shutdown.send(());
    }

    #[rstest]
    #[tokio::test]
    async fn replies_timeout_for_idle_socket() {
        let (addr, shutdown) = start_server(Duration::from_millis(100));
        let mut stream = TcpStream::connect(addr).await.expect("connect");

        let reply = timeout(Duration::from_secs(1), recv_reply(&mut stream))
            .await
            .expect("reply timed out in test")
            .expect("handshake reply");
        assert_eq!(
            u32::from_be_bytes(
                reply[4..8]
                    .try_into()
                    .expect("convert reply slice to array (timeout)")
            ),
            HANDSHAKE_ERR_TIMEOUT
        );
        let _ = shutdown.send(());
    }
}

#[cfg(test)]
#[path = "handshake_bdd.rs"]
mod bdd;
