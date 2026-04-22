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

/// Stateful decoder half of `HotlineFrameCodec`, tracking active fragment series.
#[doc(hidden)]
pub struct HotlineFrameDecoder {
    series: InboundSeriesTracker,
}

impl HotlineFrameDecoder {
    /// Create a new decoder.
    #[rustfmt::skip]
    const fn new() -> Self { Self { series: InboundSeriesTracker::new() } }
}

impl Decoder for HotlineFrameDecoder {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let Some((header, payload)) = super::take_hotline_frame(src)? else {
            return Ok(None);
        };

        let envelope_payload = if self.series.has_active_series() {
            self.series.continue_series(&header, &payload)?
        } else {
            self.series.start(&header, &payload)?
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

/// Encoder half of `HotlineFrameCodec` that delegates to `HotlineCodec`.
#[doc(hidden)]
pub struct HotlineFrameEncoder {
    inner: HotlineCodec,
}

impl HotlineFrameEncoder {
    /// Create a new encoder.
    #[rustfmt::skip]
    fn new() -> Self { Self { inner: HotlineCodec::new() } }
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

/// Tracker for one in-progress multi-fragment Hotline series.
struct InboundSeriesTracker {
    state: Option<InboundSeriesState>,
}

impl InboundSeriesTracker {
    /// Create a tracker with no active series.
    const fn new() -> Self { Self { state: None } }

    /// Returns `true` when a fragment series is in progress.
    const fn has_active_series(&self) -> bool { self.state.is_some() }

    /// Record the first fragment, open an active series if needed, and return its payload.
    fn start(
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
            self.state = Some(InboundSeriesState {
                first_header: header.clone(),
                message_key,
                remaining,
                next_sequence: FrameSequence(1),
                deadline: Instant::now() + SERIES_TIMEOUT,
            });
        }

        first_frame_payload(message_key, header, payload)
    }

    /// Validate and advance an active series by one continuation fragment.
    fn continue_series(
        &mut self,
        header: &crate::transaction::FrameHeader,
        payload: &[u8],
    ) -> Result<Vec<u8>, io::Error> {
        self.ensure_active_series()?;
        self.fail_if_timed_out()?;
        self.validate_fragment_consistency(header)?;

        let data_size = payload.len();
        let active_series = self.active_state()?;
        let remaining = active_series.remaining;
        if data_size > remaining {
            self.clear();
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "fragment exceeds remaining payload size",
            ));
        }

        let is_last = data_size == remaining;
        let message_key = active_series.message_key;
        let sequence = active_series.next_sequence;
        if is_last {
            self.clear();
        } else {
            let next_sequence = sequence.checked_increment().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "fragment sequence overflow while tracking continuation",
                )
            })?;
            let active_series_mut = self.active_state_mut()?;
            active_series_mut.remaining -= data_size;
            active_series_mut.next_sequence = next_sequence;
            active_series_mut.deadline = Instant::now() + SERIES_TIMEOUT;
        }

        continuation_frame_payload(message_key, sequence, is_last, payload)
    }

    /// Return an error if no fragment series is currently active.
    fn ensure_active_series(&self) -> Result<(), io::Error> { self.active_state().map(drop) }

    /// Clear state and return an error if the series deadline has elapsed.
    fn fail_if_timed_out(&mut self) -> Result<(), io::Error> {
        let has_timed_out = self
            .state
            .as_ref()
            .is_some_and(|series| Instant::now() > series.deadline);
        if has_timed_out {
            self.clear();
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "fragmented Hotline request timed out waiting for continuation",
            ))
        } else {
            Ok(())
        }
    }

    /// Delegate header-consistency checks while preserving active state on failure.
    fn validate_fragment_consistency(
        &self,
        header: &crate::transaction::FrameHeader,
    ) -> Result<(), io::Error> {
        let active_series = self.active_state()?;
        // Malformed continuation headers do not consume active-series state.
        super::validate_fragment_consistency(&active_series.first_header, header)
            .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))
    }

    /// Return a shared reference to the active series state, or an error.
    fn active_state(&self) -> Result<&InboundSeriesState, io::Error> {
        self.state.as_ref().ok_or_else(missing_active_series_error)
    }

    /// Return a mutable reference to the active series state, or an error.
    fn active_state_mut(&mut self) -> Result<&mut InboundSeriesState, io::Error> {
        self.state.as_mut().ok_or_else(missing_active_series_error)
    }

    /// Clear the active series, discarding in-progress reassembly state.
    const fn clear(&mut self) { self.state = None; }
}

/// Construct the standard error used when no continuation series is active.
fn missing_active_series_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "continuation fragment arrived without an active series",
    )
}

/// Per-series state held while a multi-fragment Hotline transaction is assembled.
#[derive(Debug)]
struct InboundSeriesState {
    /// Header captured from the first fragment for later consistency checks.
    first_header: crate::transaction::FrameHeader,
    /// Stable message key derived from the first fragment.
    message_key: wireframe::message_assembler::MessageKey,
    /// Number of body bytes still expected from later fragments.
    remaining: usize,
    /// Sequence number required for the next continuation fragment.
    next_sequence: FrameSequence,
    /// Deadline by which the next continuation fragment must arrive.
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
