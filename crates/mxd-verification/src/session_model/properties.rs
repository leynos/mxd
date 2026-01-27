//! Safety and reachability property definitions for the session gating model.
//!
//! This module defines the invariants that Stateright verifies hold across all
//! reachable states. Properties are divided into:
//!
//! - **Safety properties** ("always"): Must hold in every reachable state.
//! - **Reachability properties** ("sometimes"): Must be reachable in at least one path.
//!
//! The core safety invariant is that privileged operations cannot complete
//! without proper authentication and authorization.

use stateright::Property;

use super::{
    SessionModel,
    state::{Effect, SystemState},
};

#[must_use]
fn has_prior_authentication(state: &SystemState, client: usize, before_index: usize) -> bool {
    state
        .effects
        .iter()
        .take(before_index)
        .any(|e| matches!(e, Effect::Authenticated { client: c, .. } if *c == client))
}

#[must_use]
fn first_privileged_index(state: &SystemState, client: usize) -> Option<usize> {
    state.effects.iter().position(|e| {
        matches!(
            e,
            Effect::PrivilegedEffectCompleted { client: c, .. } if *c == client
        )
    })
}

#[must_use]
fn auth_precedes_first_privileged_effect(state: &SystemState, client: usize) -> bool {
    let Some(priv_idx) = first_privileged_index(state, client) else {
        return true;
    };
    let Some(auth_idx) = state.first_auth_index(client) else {
        return false;
    };
    auth_idx < priv_idx
}

#[must_use]
fn auth_precedes_privileged_effects(state: &SystemState) -> bool {
    for client_idx in 0..state.sessions.len() {
        if !auth_precedes_first_privileged_effect(state, client_idx) {
            return false;
        }
    }
    true
}

/// Safety property: No privileged effect without prior authentication.
///
/// Verifies that every `PrivilegedEffectCompleted` in the effect history is
/// preceded by an `Authenticated` event for the same client. This prevents
/// privilege escalation through message reordering.
#[must_use]
pub fn no_privileged_effect_without_auth() -> Property<SessionModel> {
    Property::always(
        "no privileged effect without authentication",
        |_model, state: &SystemState| {
            // Check that every privileged effect has a preceding authentication
            for (idx, effect) in state.effects.iter().enumerate() {
                let Effect::PrivilegedEffectCompleted { client, .. } = effect else {
                    continue;
                };
                // Look for an authentication event for this client before this effect.
                if !has_prior_authentication(state, *client, idx) {
                    return false;
                }
            }
            true
        },
    )
}

/// Safety property: No privileged effect without the required privilege.
///
/// Verifies that every `PrivilegedEffectCompleted` corresponds to a request
/// whose required privilege was held by the client at the time. Combined with
/// the authentication property, this ensures full privilege enforcement.
#[must_use]
pub fn no_privileged_effect_without_required_privilege() -> Property<SessionModel> {
    Property::always(
        "no privileged effect without required privilege",
        |_model, state: &SystemState| {
            state.effects.iter().all(|effect| match effect {
                Effect::PrivilegedEffectCompleted {
                    privilege,
                    session_privileges,
                    ..
                } => (*session_privileges & *privilege) == *privilege,
                _ => true,
            })
        },
    )
}

/// Safety property: Authentication precedes any privileged effect for a client.
///
/// A temporal safety property that verifies the ordering invariant: in the
/// effect history, any `PrivilegedEffectCompleted` for client N must come
/// after an `Authenticated` for client N.
#[must_use]
pub fn authentication_precedes_privileged_effect() -> Property<SessionModel> {
    Property::always(
        "authentication precedes privileged effect",
        |_model, state: &SystemState| auth_precedes_privileged_effects(state),
    )
}

fn state_has_effect(state: &SystemState, predicate: fn(&Effect) -> bool) -> bool {
    state.effects.iter().any(predicate)
}

const fn is_rejected_unauthenticated(effect: &Effect) -> bool {
    matches!(effect, Effect::RejectedUnauthenticated { .. })
}

const fn is_privileged_effect(effect: &Effect) -> bool {
    matches!(effect, Effect::PrivilegedEffectCompleted { .. })
}

const fn is_rejected_insufficient_privilege(effect: &Effect) -> bool {
    matches!(effect, Effect::RejectedInsufficientPrivilege { .. })
}

/// Reachability property name: reject unauthenticated request.
pub const CAN_REJECT_UNAUTHENTICATED_NAME: &str = "can reject unauthenticated request";

/// Reachability property name: complete privileged operation.
pub const CAN_COMPLETE_PRIVILEGED_OPERATION_NAME: &str = "can complete privileged operation";

/// Reachability property name: reject insufficient privilege.
pub const CAN_REJECT_INSUFFICIENT_PRIVILEGE_NAME: &str = "can reject insufficient privilege";

/// Reachability property name: observe multiple queued messages.
pub const CAN_DELIVER_OUT_OF_ORDER_NAME: &str = "can have multiple queued messages";

fn rejected_unauthenticated_condition(_model: &SessionModel, state: &SystemState) -> bool {
    state_has_effect(state, is_rejected_unauthenticated)
}

fn privileged_operation_condition(_model: &SessionModel, state: &SystemState) -> bool {
    state_has_effect(state, is_privileged_effect)
}

fn rejected_insufficient_privilege_condition(_model: &SessionModel, state: &SystemState) -> bool {
    state_has_effect(state, is_rejected_insufficient_privilege)
}

/// Helper function to create a reachability property that checks for a
/// specific effect.
fn has_effect_sometimes(
    description: &'static str,
    condition: fn(&SessionModel, &SystemState) -> bool,
) -> Property<SessionModel> {
    Property::sometimes(description, condition)
}

/// Reachability property: The model can reject unauthenticated requests.
///
/// Verifies that there exists a path where a `RejectedUnauthenticated` effect
/// occurs. This confirms the model exercises the rejection code path.
#[must_use]
pub fn can_reject_unauthenticated() -> Property<SessionModel> {
    has_effect_sometimes(
        CAN_REJECT_UNAUTHENTICATED_NAME,
        rejected_unauthenticated_condition,
    )
}

/// Reachability property: The model can complete a privileged operation.
///
/// Verifies that there exists a path where a `PrivilegedEffectCompleted`
/// effect occurs. This confirms the model exercises the success code path.
#[must_use]
pub fn can_complete_privileged_operation() -> Property<SessionModel> {
    has_effect_sometimes(
        CAN_COMPLETE_PRIVILEGED_OPERATION_NAME,
        privileged_operation_condition,
    )
}

/// Reachability property: The model can reject due to insufficient privileges.
///
/// Verifies that there exists a path where a `RejectedInsufficientPrivilege`
/// effect occurs. This confirms the model exercises the privilege check path.
#[must_use]
pub fn can_reject_insufficient_privilege() -> Property<SessionModel> {
    has_effect_sometimes(
        CAN_REJECT_INSUFFICIENT_PRIVILEGE_NAME,
        rejected_insufficient_privilege_condition,
    )
}

/// Reachability property: The model can deliver messages out of order.
///
/// Verifies that there exists a path where a client has multiple queued
/// messages. This is the necessary precondition for out-of-order delivery
/// because the model allows selecting any queue index when delivering.
#[must_use]
pub fn can_deliver_out_of_order() -> Property<SessionModel> {
    // This is verified by the model's action generation allowing any queue
    // index to be selected. We verify by checking that we can reach a state
    // where a queue has multiple messages (precondition for out-of-order).
    Property::sometimes(
        CAN_DELIVER_OUT_OF_ORDER_NAME,
        |_model, state: &SystemState| state.queues.iter().any(|q| q.len() > 1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_precedes_priv_effect_passes_empty_state() {
        let state = SystemState::new(2);
        let prop = authentication_precedes_privileged_effect();
        // Property::always returns a closure; we test the condition directly.
        assert!(auth_precedes_privileged_effects(&state));

        // Check that the property name is correct
        assert!(prop.name.contains("authentication"));
    }

    #[test]
    fn auth_precedes_priv_effect_passes_with_auth_then_priv() {
        use crate::session_model::state::RequestType;

        let mut state = SystemState::new(2);
        state.effects.push(Effect::Authenticated {
            client: 0,
            user_id: 42,
        });
        state.effects.push(Effect::PrivilegedEffectCompleted {
            client: 0,
            request: RequestType::GetFileList,
            privilege: 4,
            session_privileges: 4,
        });

        assert!(auth_precedes_privileged_effects(&state));
    }

    #[test]
    fn auth_precedes_priv_effect_fails_with_priv_before_auth() {
        use crate::session_model::state::RequestType;

        let mut state = SystemState::new(2);
        // Privileged effect before authentication — should fail
        state.effects.push(Effect::PrivilegedEffectCompleted {
            client: 0,
            request: RequestType::GetFileList,
            privilege: 4,
            session_privileges: 4,
        });
        state.effects.push(Effect::Authenticated {
            client: 0,
            user_id: 42,
        });

        assert!(!auth_precedes_privileged_effects(&state));
    }

    #[test]
    fn can_reject_unauthenticated_property() {
        use crate::session_model::state::RequestType;

        let mut state = SystemState::new(2);
        state.effects.push(Effect::RejectedUnauthenticated {
            client: 0,
            request: RequestType::GetFileList,
        });

        let has_rejection = state
            .effects
            .iter()
            .any(|e| matches!(e, Effect::RejectedUnauthenticated { .. }));
        assert!(has_rejection);
    }

    #[test]
    fn no_privileged_effect_without_required_privilege_fails_when_missing() {
        use crate::session_model::{
            privileges::{DOWNLOAD_FILE, NO_PRIVILEGES},
            state::RequestType,
        };

        let mut state = SystemState::new(1);
        state.effects.push(Effect::PrivilegedEffectCompleted {
            client: 0,
            request: RequestType::GetFileList,
            privilege: DOWNLOAD_FILE,
            session_privileges: NO_PRIVILEGES,
        });

        let has_required_privilege = state.effects.iter().all(|effect| match effect {
            Effect::PrivilegedEffectCompleted {
                privilege,
                session_privileges,
                ..
            } => (*session_privileges & *privilege) == *privilege,
            _ => true,
        });

        assert!(!has_required_privilege);
    }
}
