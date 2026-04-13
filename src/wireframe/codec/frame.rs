//! Wireframe frame codec for Hotline transactions.
//!
//! This adapter bridges the Hotline framing logic to wireframe's `FrameCodec`
//! interface by converting between raw Hotline transaction bytes and
//! bincode-encoded `Envelope` payloads. Inbound decoding surfaces physical
//! Hotline frames to Wireframe's protocol-level `MessageAssembler`, while
//! outbound encoding preserves the existing logical transaction writer.

use std::{
    io,
    time::{Duration, Instant},
};

use bytes::{Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
use wireframe::{
    app::{Envelope, Packet},
    codec::FrameCodec,
    message::Message,
    message_assembler::FrameSequence,
};

use super::{HotlineCodec, HotlineTransaction};
use crate::{
    transaction::parse_transaction,
    wireframe::{
        message_assembly::{
            HOTLINE_LOGICAL_MESSAGE_BYTES,
            continuation_frame_payload,
            first_frame_payload,
            message_key_for,
        },
        route_ids::route_id_for,
    },
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
    series: Option<InboundSeriesState>,
}

impl HotlineFrameDecoder {
    const fn new() -> Self { Self { series: None } }

    fn build_first_frame_payload(
        &mut self,
        header: &crate::transaction::FrameHeader,
        payload: &[u8],
    ) -> Result<Vec<u8>, io::Error> {
        let message_key = message_key_for(header);
        let mut remaining = usize::try_from(header.total_size).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "frame total size too large")
        })?;
        remaining = remaining.checked_sub(payload.len()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "first fragment exceeds declared total size",
            )
        })?;
        if remaining > 0 {
            self.series = Some(InboundSeriesState {
                first_header: header.clone(),
                message_key,
                remaining,
                next_sequence: FrameSequence(1),
                deadline: Instant::now() + SERIES_TIMEOUT,
            });
        }
        first_frame_payload(message_key, header, payload)
    }

    fn build_continuation_payload(
        &mut self,
        header: &crate::transaction::FrameHeader,
        payload: &[u8],
    ) -> Result<Vec<u8>, io::Error> {
        let Some(series) = self.series.as_mut() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "continuation fragment arrived without an active series",
            ));
        };
        if Instant::now() > series.deadline {
            self.series = None;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "fragmented Hotline request timed out waiting for continuation",
            ));
        }
        super::validate_fragment_consistency(&series.first_header, header)
            .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

        let data_size = payload.len();
        if data_size > series.remaining {
            self.series = None;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "fragment exceeds remaining payload size",
            ));
        }

        let is_last = data_size == series.remaining;
        let message_key = series.message_key;
        let sequence = series.next_sequence;
        if is_last {
            self.series = None;
        } else {
            series.remaining -= data_size;
            series.next_sequence = sequence.checked_increment().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "fragment sequence overflow while tracking continuation",
                )
            })?;
            series.deadline = Instant::now() + SERIES_TIMEOUT;
        }

        continuation_frame_payload(message_key, sequence, is_last, payload)
    }
}

impl Decoder for HotlineFrameDecoder {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let Some((header, payload)) = super::take_hotline_frame(src)? else {
            return Ok(None);
        };

        let envelope_payload = if self.series.is_some() {
            self.build_continuation_payload(&header, &payload)?
        } else {
            self.build_first_frame_payload(&header, &payload)?
        };
        let envelope = Envelope::new(
            route_id_for(header.ty),
            Some(u64::from(header.id)),
            envelope_payload,
        );
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
        let payload = envelope.into_parts().into_payload();
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

    fn wrap_payload(&self, payload: Bytes) -> Self::Frame { payload.to_vec() }

    fn max_frame_length(&self) -> usize { HOTLINE_LOGICAL_MESSAGE_BYTES }
}

/// Timeout for receiving the next physical fragment in one Hotline series.
///
/// This mirrors the legacy transaction reader's five-second I/O timeout for
/// multi-frame payload progress without changing the server's overall idle
/// connection policy.
const SERIES_TIMEOUT: Duration = crate::transaction::IO_TIMEOUT;

#[derive(Debug)]
struct InboundSeriesState {
    first_header: crate::transaction::FrameHeader,
    message_key: wireframe::message_assembler::MessageKey,
    remaining: usize,
    next_sequence: FrameSequence,
    deadline: Instant,
}

#[cfg(test)]
mod tests {
    //! Tests cover `HotlineFrameCodec` payload wrapping, inbound fragment
    //! metadata, and logical message-budget invariants.

    use bytes::{Bytes, BytesMut};
    use rstest::{fixture, rstest};
    use tokio_util::codec::Decoder as _;
    use wireframe::{
        app::{Envelope, Packet},
        codec::FrameCodec,
        correlation::CorrelatableFrame,
        message::Message,
    };

    use super::HotlineFrameCodec;
    use crate::{
        transaction::{FrameHeader, HEADER_LEN, MAX_PAYLOAD_SIZE},
        wireframe::test_helpers::fragmented_transaction_bytes,
    };

    #[fixture]
    fn codec() -> HotlineFrameCodec {
        // Provide a fresh codec instance per rstest case.
        HotlineFrameCodec::new()
    }

    #[rstest]
    #[case(Bytes::from(vec![0u8, 1u8, 2u8, 3u8, 4u8]), vec![0u8, 1u8, 2u8, 3u8, 4u8])]
    #[case(Bytes::new(), Vec::new())]
    fn wrap_payload_cases(
        codec: HotlineFrameCodec,
        #[case] bytes: Bytes,
        #[case] expected: Vec<u8>,
    ) {
        let frame = codec.wrap_payload(bytes);

        assert_eq!(frame, expected);
    }

    #[rstest]
    #[case(vec![10u8, 20u8, 30u8], vec![10u8, 20u8, 30u8])]
    #[case(Vec::new(), Vec::new())]
    fn frame_payload_cases(#[case] data: Vec<u8>, #[case] expected: Vec<u8>) {
        let slice = HotlineFrameCodec::frame_payload(&data);

        assert_eq!(slice, expected.as_slice());
    }

    #[test]
    fn max_frame_length_matches_logical_message_budget() {
        let codec = HotlineFrameCodec::new();

        assert_eq!(codec.max_frame_length(), HEADER_LEN + MAX_PAYLOAD_SIZE);
    }

    #[test]
    fn codec_round_trip_payload_unchanged() {
        let codec = HotlineFrameCodec::new();
        let original = vec![0xabu8, 0xcdu8, 0xefu8];
        let bytes = Bytes::from(original.clone());

        let frame = codec.wrap_payload(bytes);
        let extracted = HotlineFrameCodec::frame_payload(&frame);

        assert_eq!(extracted, original);
    }

    #[test]
    fn decoder_emits_first_then_continuation_payloads_for_fragmented_request() {
        let header = FrameHeader {
            flags: 0,
            is_reply: 0,
            ty: 107,
            id: 44,
            error: 0,
            total_size: 6,
            data_size: 6,
        };
        let fragments = fragmented_transaction_bytes(&header, b"abcdef", 4).expect("fragments");
        let mut bytes = BytesMut::new();
        let mut decoder = HotlineFrameCodec::new().decoder();

        bytes.extend_from_slice(&fragments[0]);
        let first = decoder
            .decode(&mut bytes)
            .expect("decode first frame")
            .expect("first frame payload");
        let (first_env, _) = Envelope::from_bytes(&first).expect("decode first envelope");
        assert_eq!(
            first_env.id(),
            crate::wireframe::route_ids::route_id_for(107)
        );
        assert_eq!(first_env.correlation_id(), Some(44));

        bytes.extend_from_slice(&fragments[1]);
        let second = decoder
            .decode(&mut bytes)
            .expect("decode continuation")
            .expect("continuation payload");
        let (second_env, _) = Envelope::from_bytes(&second).expect("decode second envelope");
        assert_eq!(
            second_env.id(),
            crate::wireframe::route_ids::route_id_for(107)
        );
        assert_eq!(second_env.correlation_id(), Some(44));
    }
}
