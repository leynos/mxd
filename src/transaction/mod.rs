//! Provides asynchronous helpers for framing, encoding, and decoding
//! transactions.
//!
//! Transactions consist of a [`FrameHeader`] followed by an optional payload
//! encoded using [`FieldId`] identifiers. The framing layer handles Hotline's
//! 20-byte header and multi-fragment payload envelope described in
//! `docs/protocol.md`.

use std::time::Duration;

pub mod errors;
pub mod frame;
pub mod params;
pub mod reader;
pub mod writer;

pub use errors::TransactionError;
pub use frame::{
    FrameHeader,
    Transaction,
    parse_transaction,
    read_u16,
    read_u32,
    write_u16,
    write_u32,
};
pub use params::{
    decode_params,
    decode_params_map,
    encode_params,
    first_param_i32,
    first_param_string,
    required_param_i32,
    required_param_string,
    validate_payload,
    validate_payload_parts,
};
pub use reader::{
    StreamingTransaction,
    TransactionFragment,
    TransactionReader,
    TransactionStreamReader,
};
pub use writer::TransactionWriter;

/// Length of a transaction frame header in bytes.
pub const HEADER_LEN: usize = 20;
/// Maximum allowed payload size for a buffered transaction.
///
/// Streaming readers and writers may be configured with larger limits when
/// handling file transfers or other large payloads.
pub const MAX_PAYLOAD_SIZE: usize = 1024 * 1024; // 1 MiB
/// Maximum data size per frame when writing.
pub const MAX_FRAME_DATA: usize = 32 * 1024; // 32 KiB
/// Default I/O timeout when reading or writing transactions.
pub const IO_TIMEOUT: Duration = Duration::from_secs(5);
