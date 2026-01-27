//! Stateright model for verifying session gating and privilege enforcement.
//!
//! This module implements a formal verification model that explores all
//! possible interleavings of login, logout, and request processing across
//! multiple concurrent client sessions. The model verifies that:
//!
//! 1. **Safety**: Privileged operations cannot complete without proper authentication and
//!    authorization.
//! 2. **Temporal ordering**: Authentication must precede any privileged effect.
//! 3. **Out-of-order resilience**: Message reordering cannot bypass security.
//!
//! # Example
//!
//! ```
//! use mxd_verification::session_model::SessionModel;
//! use stateright::Checker;
//!
//! let model = SessionModel::default();
//! let checker = model.checker().spawn_bfs().join();
//! assert!(checker.is_done());
//! ```
//!
//! # Model configuration
//!
//! The model is parameterized by:
//! - `num_clients`: Number of concurrent client sessions (default: 2)
//! - `max_queue_depth`: Maximum messages queued per client (default: 2)
//! - `user_ids`: Pool of user IDs for login actions
//! - `privilege_sets`: Pool of privilege sets for login actions
//!
//! Conservative defaults keep the state space tractable while still exploring
//! essential interleavings.

pub mod actions;
pub mod privileges;
pub mod properties;
pub mod state;

use stateright::{Model, Property};

use self::{
    actions::{Action, apply_action},
    privileges::{DEFAULT_USER_PRIVILEGES, NO_PRIVILEGES},
    properties::{
        authentication_precedes_privileged_effect,
        can_complete_privileged_operation,
        can_deliver_out_of_order,
        can_reject_insufficient_privilege,
        can_reject_unauthenticated,
        no_privileged_effect_without_auth,
        no_privileged_effect_without_required_privilege,
    },
    state::{ModelSession, RequestType, SystemState},
};

/// Configuration for the session gating verification model.
///
/// Use [`SessionModel::default()`] for conservative defaults suitable for
/// automated testing, or construct a custom configuration for exploratory
/// verification.
#[derive(Clone, Debug)]
pub struct SessionModel {
    /// Number of concurrent client sessions to model.
    pub num_clients: usize,
    /// Maximum number of messages that can be queued per client.
    pub max_queue_depth: usize,
    /// Pool of user IDs that can be used in login actions.
    pub user_ids: Vec<u32>,
    /// Pool of privilege sets that can be assigned on login.
    pub privilege_sets: Vec<u64>,
}

impl Default for SessionModel {
    fn default() -> Self {
        Self {
            num_clients: 2,
            max_queue_depth: 2,
            user_ids: vec![1, 2],
            privilege_sets: vec![
                NO_PRIVILEGES,           // User with no privileges
                DEFAULT_USER_PRIVILEGES, // Standard user
            ],
        }
    }
}

impl SessionModel {
    /// Creates a new model with the specified number of clients.
    ///
    /// Values below one are saturated to one to keep the model valid.
    #[must_use]
    pub fn with_clients(num_clients: usize) -> Self {
        // Stateright expects at least one client; saturate zero to one.
        let bounded_clients = num_clients.clamp(1, u32::MAX as usize);
        let max_user_id = u32::try_from(bounded_clients).unwrap_or(u32::MAX);
        Self {
            num_clients: bounded_clients,
            user_ids: (1..=max_user_id).collect(),
            ..Default::default()
        }
    }

    /// Creates a minimal model for quick verification (single client).
    #[must_use]
    pub fn minimal() -> Self {
        Self {
            num_clients: 1,
            max_queue_depth: 2,
            user_ids: vec![1],
            privilege_sets: vec![NO_PRIVILEGES, DEFAULT_USER_PRIVILEGES],
        }
    }

    fn push_login_actions(&self, state: &SystemState, actions: &mut Vec<Action>, client: usize) {
        let Some(session) = state.session(client) else {
            return;
        };
        if session.is_authenticated() {
            return;
        }

        for &user_id in &self.user_ids {
            for &privileges in &self.privilege_sets {
                actions.push(Action::Login {
                    client,
                    user_id,
                    privileges,
                });
            }
        }
    }

    fn push_logout_action(state: &SystemState, actions: &mut Vec<Action>, client: usize) {
        if state
            .session(client)
            .is_some_and(ModelSession::is_authenticated)
        {
            actions.push(Action::Logout { client });
        }
    }

    fn push_send_request_actions(
        &self,
        state: &SystemState,
        actions: &mut Vec<Action>,
        client: usize,
    ) {
        let Some(queue) = state.queue(client) else {
            return;
        };
        if queue.len() >= self.max_queue_depth {
            return;
        }

        actions.extend(
            RequestType::all()
                .iter()
                .copied()
                .map(|request| Action::SendRequest { client, request }),
        );
    }

    fn push_deliver_actions(state: &SystemState, actions: &mut Vec<Action>, client: usize) {
        let Some(queue) = state.queue(client) else {
            return;
        };
        for queue_index in 0..queue.len() {
            actions.push(Action::DeliverRequest {
                client,
                queue_index,
            });
        }
    }
}

impl Model for SessionModel {
    type State = SystemState;
    type Action = Action;

    fn init_states(&self) -> Vec<Self::State> {
        // Single initial state: all clients unauthenticated with empty queues
        vec![SystemState::new(self.num_clients)]
    }

    fn actions(&self, state: &Self::State, actions: &mut Vec<Self::Action>) {
        for client in 0..self.num_clients {
            self.push_login_actions(state, actions, client);
            Self::push_logout_action(state, actions, client);
            self.push_send_request_actions(state, actions, client);
            Self::push_deliver_actions(state, actions, client);
        }
    }

    fn next_state(&self, state: &Self::State, action: Self::Action) -> Option<Self::State> {
        Some(apply_action(state, &action))
    }

    fn properties(&self) -> Vec<Property<Self>> {
        vec![
            // Safety properties
            no_privileged_effect_without_auth(),
            no_privileged_effect_without_required_privilege(),
            authentication_precedes_privileged_effect(),
            // Reachability properties
            can_reject_unauthenticated(),
            can_complete_privileged_operation(),
            can_reject_insufficient_privilege(),
            can_deliver_out_of_order(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use stateright::{Checker, HasDiscoveries};

    use super::*;
    use crate::session_model::properties::{
        CAN_COMPLETE_PRIVILEGED_OPERATION_NAME,
        CAN_DELIVER_OUT_OF_ORDER_NAME,
        CAN_REJECT_INSUFFICIENT_PRIVILEGE_NAME,
        CAN_REJECT_UNAUTHENTICATED_NAME,
    };

    const MIN_STATE_COUNT: usize = 10;
    const TARGET_MAX_DEPTH: usize = 6;
    const TARGET_STATE_COUNT: usize = 1500;

    fn verify_bounded(model: SessionModel) -> impl stateright::Checker<SessionModel> {
        let reachability = reachability_property_names();
        model
            .checker()
            .target_max_depth(TARGET_MAX_DEPTH)
            .target_state_count(TARGET_STATE_COUNT)
            .finish_when(HasDiscoveries::AllOf(reachability))
            .spawn_bfs()
            .join()
    }

    fn reachability_property_names() -> BTreeSet<&'static str> {
        [
            CAN_REJECT_UNAUTHENTICATED_NAME,
            CAN_COMPLETE_PRIVILEGED_OPERATION_NAME,
            CAN_REJECT_INSUFFICIENT_PRIVILEGE_NAME,
            CAN_DELIVER_OUT_OF_ORDER_NAME,
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn default_model_has_reasonable_config() {
        let model = SessionModel::default();
        assert_eq!(model.num_clients, 2);
        assert_eq!(model.max_queue_depth, 2);
        assert!(!model.user_ids.is_empty());
        assert!(!model.privilege_sets.is_empty());
    }

    #[test]
    fn minimal_model_has_single_client() {
        let model = SessionModel::minimal();
        assert_eq!(model.num_clients, 1);
    }

    #[test]
    fn with_clients_scales_user_ids() {
        let model = SessionModel::with_clients(3);
        assert_eq!(model.num_clients, 3);
        assert_eq!(model.user_ids.len(), 3);
    }

    #[test]
    fn init_states_returns_single_state() {
        let model = SessionModel::default();
        let states = model.init_states();
        assert_eq!(states.len(), 1);
        let state = states.first().expect("state exists");
        assert_eq!(state.num_clients(), model.num_clients);
    }

    #[test]
    fn actions_generated_for_initial_state() {
        let model = SessionModel::default();
        let state = SystemState::new(model.num_clients);
        let mut actions = Vec::new();
        model.actions(&state, &mut actions);

        // Should have login actions and send request actions
        // No logout or deliver actions (not authenticated, queues empty)
        assert!(!actions.is_empty());
        assert!(actions.iter().any(|a| matches!(a, Action::Login { .. })));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::SendRequest { .. }))
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::Logout { .. })));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::DeliverRequest { .. }))
        );
    }

    #[test]
    fn properties_includes_safety_and_reachability() {
        let model = SessionModel::default();
        let props = model.properties();

        // Should have at least 3 safety + 4 reachability properties
        assert!(props.len() >= 7);

        // Check property names
        assert!(props.iter().any(|p| p.name.contains("authentication")));
        assert!(props.iter().any(|p| p.name.contains("privileged")));
        assert!(props.iter().any(|p| p.name.contains("reject")));
    }

    #[test]
    fn minimal_model_verifies_successfully() {
        let checker = verify_bounded(SessionModel::minimal());
        assert!(checker.unique_state_count() >= MIN_STATE_COUNT);
    }

    #[test]
    fn model_explores_multiple_states() {
        let checker = verify_bounded(SessionModel::default());
        assert!(
            checker.unique_state_count() >= MIN_STATE_COUNT,
            "Expected >= {MIN_STATE_COUNT} states, got {}",
            checker.unique_state_count()
        );
    }
}
