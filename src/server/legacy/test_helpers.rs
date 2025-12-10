//! Shared helper utilities for exercising legacy server logic in tests.

#![allow(clippy::big_endian_bytes, reason = "network protocol uses big-endian")]

use std::time::Duration;

use diesel_async::pooled_connection::{AsyncDieselConnectionManager, bb8::Pool};

use crate::{
    db::{DbConnection, DbPool},
    protocol,
};

pub(super) fn dummy_pool() -> DbPool {
    let manager =
        AsyncDieselConnectionManager::<DbConnection>::new("postgres://example.invalid/mxd-test");
    Pool::builder()
        .max_size(1)
        .min_idle(Some(0))
        .idle_timeout(None::<Duration>)
        .max_lifetime(None::<Duration>)
        .test_on_check_out(false)
        .build_unchecked(manager)
}

pub(super) fn handshake_frame() -> [u8; protocol::HANDSHAKE_LEN] {
    let mut buf = [0u8; protocol::HANDSHAKE_LEN];
    buf[0..4].copy_from_slice(protocol::PROTOCOL_ID);
    buf[8..10].copy_from_slice(&protocol::VERSION.to_be_bytes());
    buf
}
