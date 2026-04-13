//! Explicit Wireframe memory-budget sizing for Hotline request assembly.
//!
//! The transport adapter keeps fragmented Hotline request handling limited to
//! one in-flight logical transaction per connection, matching the legacy
//! sequential framing model. All three budget dimensions therefore collapse to
//! the same logical transaction envelope: a normalized 20-byte header plus the
//! maximum 1 MiB assembled payload.

use std::num::NonZeroUsize;

use wireframe::app::{BudgetBytes, MemoryBudgets};

use crate::wireframe::message_assembly::HOTLINE_LOGICAL_MESSAGE_BYTES;

/// Build the explicit Wireframe memory budgets for the Hotline adapter.
#[must_use]
pub(crate) fn explicit_memory_budgets() -> MemoryBudgets {
    let logical_message_bytes = non_zero(HOTLINE_LOGICAL_MESSAGE_BYTES);
    let budget = BudgetBytes::new(logical_message_bytes);
    MemoryBudgets::new(budget, budget, budget)
}

fn non_zero(bytes: usize) -> NonZeroUsize { NonZeroUsize::new(bytes).unwrap_or(NonZeroUsize::MIN) }

#[cfg(test)]
mod tests {
    //! Unit coverage for explicit Hotline memory-budget derivation.

    use super::*;

    #[test]
    fn budgets_match_one_full_hotline_logical_message() {
        let budgets = explicit_memory_budgets();

        assert_eq!(
            budgets.bytes_per_message().as_usize(),
            HOTLINE_LOGICAL_MESSAGE_BYTES
        );
        assert_eq!(
            budgets.bytes_per_connection().as_usize(),
            HOTLINE_LOGICAL_MESSAGE_BYTES
        );
        assert_eq!(
            budgets.bytes_in_flight().as_usize(),
            HOTLINE_LOGICAL_MESSAGE_BYTES
        );
    }

    #[test]
    fn budgets_preserve_non_zero_and_relation_invariants() {
        let budgets = explicit_memory_budgets();

        assert!(budgets.bytes_per_message().as_usize() > 0);
        assert_eq!(
            budgets.bytes_per_message().as_usize(),
            budgets.bytes_per_connection().as_usize()
        );
        assert_eq!(
            budgets.bytes_per_connection().as_usize(),
            budgets.bytes_in_flight().as_usize()
        );
    }
}
