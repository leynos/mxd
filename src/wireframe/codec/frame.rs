//! Wireframe frame codec for Hotline transactions.
//!
//! This adapter bridges the Hotline framing logic to wireframe's `FrameCodec`
//! interface by converting between raw Hotline transaction bytes and
//! bincode-encoded `Envelope` payloads.

use std::io;

use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};
use wireframe::{
    app::{Envelope, Packet},
    codec::FrameCodec,
    message::Message,
};

use super::{HotlineCodec, HotlineTransaction};
use crate::{
    transaction::{HEADER_LEN, MAX_FRAME_DATA, Transaction, parse_transaction},
    wireframe::route_ids::route_id_for,
};

/// Wireframe `FrameCodec` implementation for Hotline transactions.
#[derive(Clone, Debug, Default)]
pub struct HotlineFrameCodec;

impl HotlineFrameCodec {
    /// Create a new Hotline frame codec.
    #[must_use]
    pub const fn new() -> Self { Self }
}

#[doc(hidden)]
pub struct HotlineFrameDecoder {
    inner: HotlineCodec,
}

impl HotlineFrameDecoder {
    fn new() -> Self {
        Self {
            inner: HotlineCodec::new(),
        }
    }
}

impl Decoder for HotlineFrameDecoder {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let Some(tx) = self.inner.decode(src)? else {
            return Ok(None);
        };

        let header = tx.header().clone();
        let raw_tx = Transaction::from(tx).to_bytes();
        let envelope = Envelope::new(route_id_for(header.ty), Some(u64::from(header.id)), raw_tx);
        let bytes = envelope
            .to_bytes()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        Ok(Some(bytes))
    }
}

#[doc(hidden)]
pub struct HotlineFrameEncoder {
    inner: HotlineCodec,
}

impl HotlineFrameEncoder {
    fn new() -> Self {
        Self {
            inner: HotlineCodec::new(),
        }
    }
}

impl Encoder<Vec<u8>> for HotlineFrameEncoder {
    type Error = io::Error;

    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let (envelope, _) = Envelope::from_bytes(&item)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let payload = envelope.into_parts().payload();
        let parsed = parse_transaction(&payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let tx = HotlineTransaction::try_from(parsed)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        self.inner.encode(tx, dst)
    }
}

impl FrameCodec for HotlineFrameCodec {
    type Frame = Vec<u8>;
    type Decoder = HotlineFrameDecoder;
    type Encoder = HotlineFrameEncoder;

    fn decoder(&self) -> Self::Decoder { HotlineFrameDecoder::new() }

    fn encoder(&self) -> Self::Encoder { HotlineFrameEncoder::new() }

    fn frame_payload(frame: &Self::Frame) -> &[u8] { frame.as_slice() }

    fn wrap_payload(payload: Vec<u8>) -> Self::Frame { payload }

    fn max_frame_length(&self) -> usize { HEADER_LEN + MAX_FRAME_DATA }
}
