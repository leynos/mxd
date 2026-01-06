//! Wireframe handshake hooks for Hotline connections.
//!
//! This module wires the Hotline handshake semantics into the Wireframe
//! runtime by registering preamble callbacks that emit the standard 8-byte
//! reply and by enforcing the 5-second idle timeout defined in the protocol.
//! The hooks are reusable so tests can inject shorter timeouts without
//! altering production defaults.

use std::{io, time::Duration};

use bincode::error::DecodeError;
use futures_util::{FutureExt, future::BoxFuture};
use tokio::net::TcpStream;
use tracing::warn;
use wireframe::{
    app::{Packet, WireframeApp},
    codec::FrameCodec,
    serializer::Serializer,
    server::{ServerState, WireframeServer},
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
    wireframe::connection::{ConnectionContext, HandshakeMetadata, store_current_context},
};

/// Attach Hotline handshake behaviour to a [`WireframeServer`].
///
/// The returned server writes the Hotline reply on success, returns Hotline
/// error codes on decode failures, and times out idle sockets after
/// `timeout`. Tests may call this with a shorter duration to avoid slow
/// sleeps, while production code should use [`crate::protocol::HANDSHAKE_TIMEOUT`].
#[must_use]
pub fn install<F, S, Ser, Ctx, E, Codec>(
    server: WireframeServer<F, HotlinePreamble, S, Ser, Ctx, E, Codec>,
    timeout: Duration,
) -> WireframeServer<F, HotlinePreamble, S, Ser, Ctx, E, Codec>
where
    F: Fn() -> WireframeApp<Ser, Ctx, E, Codec> + Send + Sync + Clone + 'static,
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
        async move {
            let mut context = ConnectionContext::new(HandshakeMetadata::from(preamble.handshake()));
            let peer = match stream.peer_addr() {
                Ok(peer) => peer,
                Err(error) => {
                    warn!(%error, "failed to retrieve peer address during handshake");
                    return Err(error);
                }
            };
            context = context.with_peer(peer);
            store_current_context(context);
            write_handshake_reply(stream, HANDSHAKE_OK).await?;
            Ok(())
        }
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
        BincodeSerializer,
        app::{Envelope, WireframeApp},
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
        .workers(1)
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
mod bdd {
    use std::{cell::RefCell, net::SocketAddr, time::Duration};

    use rstest::fixture;
    use rstest_bdd::assert_step_ok;
    use rstest_bdd_macros::{given, scenario, then, when};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        runtime::Runtime,
        sync::oneshot,
        time::timeout,
    };

    use crate::{
        protocol::{PROTOCOL_ID, REPLY_LEN, VERSION},
        wireframe::test_helpers::preamble_bytes,
    };

    async fn perform_handshake(
        addr: SocketAddr,
        bytes: Option<Vec<u8>>,
    ) -> Result<[u8; REPLY_LEN], String> {
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        send_handshake_bytes(&mut stream, bytes).await;
        read_handshake_reply(&mut stream).await
    }

    async fn send_handshake_bytes(stream: &mut TcpStream, bytes: Option<Vec<u8>>) {
        if let Some(data) = bytes {
            stream.write_all(&data).await.expect("write handshake");
        }
    }

    async fn read_handshake_reply(stream: &mut TcpStream) -> Result<[u8; REPLY_LEN], String> {
        let mut buf = [0u8; REPLY_LEN];
        timeout(Duration::from_secs(1), stream.read_exact(&mut buf))
            .await
            .map(|res| {
                res.expect("read reply");
                buf
            })
            .map_err(|err| err.to_string())
    }

    struct HandshakeWorld {
        rt: Runtime,
        addr: RefCell<Option<SocketAddr>>,
        shutdown: RefCell<Option<oneshot::Sender<()>>>,
        reply: RefCell<Option<Result<[u8; REPLY_LEN], String>>>,
    }

    impl HandshakeWorld {
        fn new() -> Self {
            Self {
                rt: Runtime::new().expect("runtime"),
                addr: RefCell::new(None),
                shutdown: RefCell::new(None),
                reply: RefCell::new(None),
            }
        }

        fn start_server(&self) {
            let (addr, shutdown) = self
                .rt
                .block_on(async { super::tests::start_server(Duration::from_millis(100)) });
            self.addr.borrow_mut().replace(addr);
            self.shutdown.borrow_mut().replace(shutdown);
        }

        fn connect_and_maybe_send(&self, bytes: Option<Vec<u8>>) {
            let addr = self.addr.borrow().expect("server not started");
            let reply = self.rt.block_on(perform_handshake(addr, bytes));
            self.reply.borrow_mut().replace(reply);
        }

        fn reply_code(&self) -> Result<u32, String> {
            let reply = self.reply.borrow();
            let Some(reply) = reply.as_ref() else {
                return Err("missing reply".into());
            };
            reply
                .as_ref()
                .map(|buf| {
                    u32::from_be_bytes(
                        buf[4..8]
                            .try_into()
                            .expect("convert reply slice to array (bdd reply)"),
                    )
                })
                .map_err(ToString::to_string)
        }
    }

    impl Drop for HandshakeWorld {
        fn drop(&mut self) {
            if let Some(tx) = self.shutdown.borrow_mut().take() {
                let _ = tx.send(());
            }
        }
    }

    #[expect(
        clippy::allow_attributes,
        reason = "rustc compiler does not emit expected lint"
    )]
    #[allow(unused_braces, reason = "rstest-bdd macro expansion produces braces")]
    #[fixture]
    fn world() -> HandshakeWorld { HandshakeWorld::new() }

    #[given("a wireframe server handling handshakes")]
    fn given_server(world: &HandshakeWorld) { world.start_server(); }

    #[when("I send a valid Hotline handshake")]
    fn when_valid(world: &HandshakeWorld) {
        let bytes = preamble_bytes(*PROTOCOL_ID, *b"CHAT", VERSION, 0);
        world.connect_and_maybe_send(Some(bytes.to_vec()));
    }

    #[when("I send a Hotline handshake with protocol \"{tag}\" and version {version}")]
    fn when_custom(world: &HandshakeWorld, tag: String, version: u16) {
        let mut protocol = [0u8; 4];
        protocol.copy_from_slice(tag.as_bytes());
        let bytes = preamble_bytes(protocol, *b"CHAT", version, 0);
        world.connect_and_maybe_send(Some(bytes.to_vec()));
    }

    #[when("I connect without sending a handshake")]
    fn when_idle(world: &HandshakeWorld) { world.connect_and_maybe_send(None); }

    #[then("the handshake reply code is {code}")]
    fn then_code(world: &HandshakeWorld, code: u32) {
        let reply = world.reply_code();
        let value = assert_step_ok!(reply);
        assert_eq!(value, code);
    }

    #[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 0)]
    fn replies_ok(world: HandshakeWorld) { let _ = world; }

    #[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 1)]
    fn invalid_protocol(world: HandshakeWorld) { let _ = world; }

    #[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 2)]
    fn unsupported_version(world: HandshakeWorld) { let _ = world; }

    #[scenario(path = "tests/features/wireframe_handshake_hooks.feature", index = 3)]
    fn handshake_timeout(world: HandshakeWorld) { let _ = world; }
}
