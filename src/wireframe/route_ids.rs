//! Route identifiers used by the wireframe adapter.
//!
//! These constants centralise the transaction IDs registered with the
//! wireframe routing layer and provide a fallback route for unknown types.

/// Fallback route ID for unknown transaction types.
pub const FALLBACK_ROUTE_ID: u32 = 0;

/// Transaction route IDs supported by the wireframe routing layer.
pub const ROUTE_IDS: [u32; 6] = [107, 200, 370, 371, 400, 410];

/// Resolve the route ID for a transaction type.
#[must_use]
pub fn route_id_for(transaction_type: u16) -> u32 {
    let id = u32::from(transaction_type);
    if ROUTE_IDS.contains(&id) {
        id
    } else {
        FALLBACK_ROUTE_ID
    }
}
