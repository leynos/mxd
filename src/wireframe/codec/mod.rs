//! Wireframe codec for Hotline transaction framing.
//!
//! This module implements `BorrowDecode` and `Encode` for transaction frames,
//! enabling the wireframe transport to decode the 20-byte header, reassemble
//! fragmented payloads, and emit outbound frames according to `docs/protocol.md`.
//!
//! The [`framed`] submodule provides a Tokio-compatible codec, while
//! [`frame`] exposes a wireframe `FrameCodec` wrapper for the same Hotline
//! framing rules.

mod frame;
mod framed;

use bincode::{
    de::{BorrowDecode, BorrowDecoder, read::Reader},
    enc::{Encode, Encoder, write::Writer},
    error::{DecodeError, EncodeError},
};

pub use self::{frame::HotlineFrameCodec, framed::HotlineCodec};
use crate::{
    field_id::FieldId,
    transaction::{
        FrameHeader,
        HEADER_LEN,
        MAX_FRAME_DATA,
        MAX_PAYLOAD_SIZE,
        Transaction,
        TransactionError,
        encode_params,
        validate_payload_parts,
    },
};

/// Wireframe-decoded Hotline transaction.
///
/// Wraps the validated header and reassembled payload after decoding from the
/// wireframe transport layer.
///
/// **Note:** After multi-fragment reassembly, the header's `data_size` field is
/// set to `total_size` to reflect the fully-assembled payload length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotlineTransaction {
    header: FrameHeader,
    payload: Vec<u8>,
}

impl HotlineTransaction {
    /// Build a parameter-encoded request transaction.
    ///
    /// The payload is encoded using [`crate::transaction::encode_params`] and
    /// validated using the shared parameter validation rules.
    ///
    /// # Errors
    ///
    /// Returns an error if the parameter block cannot be encoded or if the
    /// resulting payload violates protocol constraints.
    pub fn request_from_params<T: AsRef<[u8]>>(
        ty: u16,
        id: u32,
        params: &[(FieldId, T)],
    ) -> Result<Self, TransactionError> {
        Self::from_params(
            FrameHeader {
                flags: 0,
                is_reply: 0,
                ty,
                id,
                error: 0,
                total_size: 0,
                data_size: 0,
            },
            params,
        )
    }

    /// Build a parameter-encoded reply transaction mirroring a request.
    ///
    /// The payload is encoded using [`crate::transaction::encode_params`] and
    /// validated using the shared parameter validation rules.
    ///
    /// # Errors
    ///
    /// Returns an error if the parameter block cannot be encoded or if the
    /// resulting payload violates protocol constraints.
    pub fn reply_from_params<T: AsRef<[u8]>>(
        req: &FrameHeader,
        error: u32,
        params: &[(FieldId, T)],
    ) -> Result<Self, TransactionError> {
        Self::from_params(
            FrameHeader {
                flags: 0,
                is_reply: 1,
                ty: req.ty,
                id: req.id,
                error,
                total_size: 0,
                data_size: 0,
            },
            params,
        )
    }

    fn from_params<T: AsRef<[u8]>>(
        mut header: FrameHeader,
        params: &[(FieldId, T)],
    ) -> Result<Self, TransactionError> {
        if header.flags != 0 {
            return Err(TransactionError::InvalidFlags);
        }
        let payload = if params.is_empty() {
            Vec::new()
        } else {
            encode_params(params)?
        };
        let total_size = payload.len();
        if total_size > MAX_PAYLOAD_SIZE {
            return Err(TransactionError::PayloadTooLarge);
        }
        header.total_size =
            u32::try_from(total_size).map_err(|_| TransactionError::PayloadTooLarge)?;
        header.data_size = header.total_size;
        validate_payload_parts(&header, &payload)?;
        Ok(Self { header, payload })
    }

    /// Return the transaction header.
    #[must_use]
    pub const fn header(&self) -> &FrameHeader { &self.header }

    /// Return the reassembled payload.
    #[must_use]
    pub fn payload(&self) -> &[u8] { &self.payload }

    /// Consume self and return the inner header and payload.
    #[must_use]
    pub fn into_parts(self) -> (FrameHeader, Vec<u8>) { (self.header, self.payload) }

    /// Construct a transaction from pre-assembled parts.
    ///
    /// This is primarily used by the Tokio codec for reassembled multi-fragment
    /// transactions. Header invariants are validated to prevent malformed
    /// frames from entering the routing pipeline.
    ///
    /// # Errors
    ///
    /// Returns an error if the header or payload violates protocol constraints.
    pub fn from_parts(header: FrameHeader, payload: Vec<u8>) -> Result<Self, TransactionError> {
        if header.flags != 0 {
            return Err(TransactionError::InvalidFlags);
        }
        if header.total_size as usize > MAX_PAYLOAD_SIZE
            || header.data_size as usize > MAX_FRAME_DATA
        {
            return Err(TransactionError::PayloadTooLarge);
        }
        let has_data_size_overflow = header.data_size > header.total_size;
        let has_inconsistent_empty_frame = header.data_size == 0 && header.total_size > 0;
        if has_data_size_overflow || has_inconsistent_empty_frame {
            return Err(TransactionError::SizeMismatch);
        }
        validate_payload_parts(&header, &payload)?;
        Ok(Self { header, payload })
    }
}

impl TryFrom<Transaction> for HotlineTransaction {
    type Error = TransactionError;

    fn try_from(mut value: Transaction) -> Result<Self, Self::Error> {
        if value.header.flags != 0 {
            return Err(TransactionError::InvalidFlags);
        }
        if value.payload.len() > MAX_PAYLOAD_SIZE {
            return Err(TransactionError::PayloadTooLarge);
        }
        validate_payload_parts(&value.header, &value.payload)?;
        // Normalise the logical header to "reassembled" form.
        value.header.data_size = value.header.total_size;
        Ok(Self {
            header: value.header,
            payload: value.payload,
        })
    }
}

impl From<HotlineTransaction> for Transaction {
    fn from(value: HotlineTransaction) -> Self {
        let (header, payload) = value.into_parts();
        Self { header, payload }
    }
}

/// Validate a frame header against protocol constraints.
///
/// # Errors
///
/// Returns a descriptive error string if validation fails.
const fn validate_header(hdr: &FrameHeader) -> Result<(), &'static str> {
    if hdr.flags != 0 {
        return Err("invalid flags: must be 0 for v1.8.5");
    }
    if hdr.total_size as usize > MAX_PAYLOAD_SIZE {
        return Err("total size exceeds maximum (1 MiB)");
    }
    if hdr.data_size as usize > MAX_FRAME_DATA {
        return Err("data size exceeds maximum (32 KiB)");
    }
    if hdr.data_size > hdr.total_size {
        return Err("data size exceeds total size");
    }
    // Empty data with non-zero total is invalid (except for single empty frame)
    if hdr.data_size == 0 && hdr.total_size > 0 {
        return Err("data size is zero but total size is non-zero");
    }
    Ok(())
}

/// Validate that a continuation fragment has consistent header fields.
///
/// # Errors
///
/// Returns a descriptive error string if header fields mutated between fragments.
const fn validate_fragment_consistency(
    first: &FrameHeader,
    next: &FrameHeader,
) -> Result<(), &'static str> {
    if next.flags != first.flags {
        return Err("header mismatch: 'flags' changed between fragments");
    }
    if next.is_reply != first.is_reply {
        return Err("header mismatch: 'is_reply' changed between fragments");
    }
    if next.ty != first.ty {
        return Err("header mismatch: 'type' changed between fragments");
    }
    if next.id != first.id {
        return Err("header mismatch: 'id' changed between fragments");
    }
    if next.error != first.error {
        return Err("header mismatch: 'error' changed between fragments");
    }
    if next.total_size != first.total_size {
        return Err("header mismatch: 'total_size' changed between fragments");
    }
    Ok(())
}

impl<'de> BorrowDecode<'de, ()> for HotlineTransaction {
    fn borrow_decode<D: BorrowDecoder<'de, Context = ()>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        // Read the first frame header
        let mut hdr_buf = [0u8; HEADER_LEN];
        decoder.reader().read(&mut hdr_buf)?;
        let first_header = FrameHeader::from_bytes(&hdr_buf);

        // Validate header constraints
        validate_header(&first_header).map_err(|msg| DecodeError::OtherString(msg.to_owned()))?;

        // Read the first fragment's data
        let mut payload = vec![0u8; first_header.data_size as usize];
        if !payload.is_empty() {
            decoder.reader().read(&mut payload)?;
        }

        // Handle multi-fragment case
        let mut accumulated = first_header.data_size;
        while accumulated < first_header.total_size {
            // Read continuation header
            decoder.reader().read(&mut hdr_buf)?;
            let next_header = FrameHeader::from_bytes(&hdr_buf);

            // Validate continuation header
            validate_fragment_consistency(&first_header, &next_header)
                .map_err(|msg| DecodeError::OtherString(msg.to_owned()))?;

            // Validate data size for continuation
            if next_header.data_size == 0 {
                return Err(DecodeError::OtherString(
                    "continuation fragment has zero data size".to_owned(),
                ));
            }
            if next_header.data_size as usize > MAX_FRAME_DATA {
                return Err(DecodeError::OtherString(
                    "continuation data size exceeds maximum (32 KiB)".to_owned(),
                ));
            }

            let remaining = first_header.total_size - accumulated;
            if next_header.data_size > remaining {
                return Err(DecodeError::OtherString(
                    "continuation data size exceeds remaining bytes".to_owned(),
                ));
            }

            // Read continuation data directly into payload
            let chunk_size = next_header.data_size as usize;
            let start = payload.len();
            payload.resize(start + chunk_size, 0);
            #[expect(
                clippy::indexing_slicing,
                reason = "range is guaranteed in-bounds after resize"
            )]
            let chunk = &mut payload[start..start + chunk_size];
            decoder.reader().read(chunk)?;
            accumulated += next_header.data_size;
        }

        // Build the final transaction with data_size = total_size (fully assembled)
        let mut header = first_header;
        header.data_size = header.total_size;

        // Note: Payload validation (transaction::validate_payload) is intentionally
        // omitted here. The codec layer handles only frame-level concerns—header
        // validation, length bounds, and multi-fragment reassembly. Semantic payload
        // validation (parameter counts, field types) is deferred to command handlers,
        // which have the context to interpret transaction types and enforce
        // application-level constraints.
        Ok(Self { header, payload })
    }
}

impl Encode for HotlineTransaction {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        fn tx_err(err: &TransactionError) -> EncodeError {
            EncodeError::OtherString(err.to_string())
        }

        if self.header.flags != 0 {
            return Err(tx_err(&TransactionError::InvalidFlags));
        }
        if self.payload.len() > MAX_PAYLOAD_SIZE {
            return Err(tx_err(&TransactionError::PayloadTooLarge));
        }
        let total_size = u32::try_from(self.payload.len())
            .map_err(|_| tx_err(&TransactionError::PayloadTooLarge))?;
        if self.header.total_size != total_size {
            return Err(tx_err(&TransactionError::SizeMismatch));
        }

        let mut hdr_buf = [0u8; HEADER_LEN];
        if self.payload.is_empty() {
            let mut header = self.header.clone();
            header.data_size = 0;
            header.write_bytes(&mut hdr_buf);
            encoder.writer().write(&hdr_buf)?;
            return Ok(());
        }

        let mut offset = 0usize;
        while offset < self.payload.len() {
            let end = (offset + MAX_FRAME_DATA).min(self.payload.len());
            let chunk = self
                .payload
                .get(offset..end)
                .ok_or_else(|| tx_err(&TransactionError::SizeMismatch))?;
            let mut header = self.header.clone();
            header.data_size = u32::try_from(chunk.len())
                .map_err(|_| tx_err(&TransactionError::PayloadTooLarge))?;
            header.write_bytes(&mut hdr_buf);
            encoder.writer().write(&hdr_buf)?;
            encoder.writer().write(chunk)?;
            offset = end;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
