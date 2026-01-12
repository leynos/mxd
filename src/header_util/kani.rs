//! Kani harnesses for reply header construction.

use super::reply_header;
use crate::transaction::FrameHeader;

#[kani::proof]
fn kani_reply_header_echoes_id() {
    let id: u32 = kani::any();
    let ty: u16 = kani::any();
    let payload_len: u16 = kani::any();
    let error_code: u32 = kani::any();

    let req = FrameHeader {
        flags: kani::any(),
        is_reply: kani::any(),
        ty,
        id,
        error: kani::any(),
        total_size: kani::any(),
        data_size: kani::any(),
    };

    let reply = reply_header(&req, error_code, usize::from(payload_len));

    kani::assert(reply.id == id, "reply id echoes request id");
    kani::assert(reply.ty == ty, "reply type echoes request type");
    kani::assert(reply.is_reply == 1, "reply flag is set");
    kani::assert(reply.flags == 0, "reply flags are zeroed");
    kani::assert(reply.error == error_code, "reply error echoes error code");
    kani::assert(
        reply.total_size == u32::from(payload_len),
        "reply total size echoes payload length",
    );
    kani::assert(
        reply.data_size == u32::from(payload_len),
        "reply data size echoes payload length",
    );
}
