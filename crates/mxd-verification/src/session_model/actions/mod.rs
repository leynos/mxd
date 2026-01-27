//! Action types and state transitions for the session gating model.
//!
//! This module defines the actions that clients can perform (login, logout,
//! send/deliver requests) and the pure transition function that computes the
//! next state given an action.

use super::state::{Effect, ModelMessage, ModelSession, RequestType, SystemState};

/// Actions that can be taken in the session model.
///
/// Each action represents a discrete event that transitions the system state.
/// Stateright explores all possible action sequences to verify properties.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Action {
    /// A client authenticates with the given user ID and privileges.
    Login {
        /// The client index performing the login.
        client: usize,
        /// The user ID being assigned.
        user_id: u32,
        /// The privilege set granted upon login.
        privileges: u64,
    },
    /// A client logs out, clearing authentication state.
    Logout {
        /// The client index logging out.
        client: usize,
    },
    /// A client sends a request (queued for later delivery).
    SendRequest {
        /// The client index sending the request.
        client: usize,
        /// The type of request being sent.
        request: RequestType,
    },
    /// A queued request is delivered and processed.
    ///
    /// The `queue_index` allows modelling out-of-order delivery by selecting
    /// which queued message to deliver next.
    DeliverRequest {
        /// The client whose queue contains the message.
        client: usize,
        /// The index within the queue of the message to deliver.
        queue_index: usize,
    },
}

impl Action {
    /// Returns the client index associated with this action.
    #[must_use]
    pub const fn client(&self) -> usize {
        match *self {
            Self::Login { client, .. }
            | Self::Logout { client }
            | Self::SendRequest { client, .. }
            | Self::DeliverRequest { client, .. } => client,
        }
    }
}

/// Applies an action to a state, returning the resulting state.
///
/// This is a pure function: it does not modify the input state.
/// The transition semantics match the privilege enforcement in the mxd server:
///
/// - **`Login`**: Sets `user_id` and `privileges` on the session, records `Authenticated` effect.
/// - **`Logout`**: Clears `user_id` and `privileges`, records `LoggedOut` effect.
/// - **`SendRequest`**: Enqueues a message in the client's queue.
/// - **`DeliverRequest`**: Removes a message from the queue and processes it, enforcing
///   authentication and privilege requirements.
#[must_use]
pub fn apply_action(state: &SystemState, action: &Action) -> SystemState {
    let mut next = state.clone();

    match *action {
        Action::Login {
            client,
            user_id,
            privileges,
        } => apply_login(&mut next, client, user_id, privileges),

        Action::Logout { client } => apply_logout(&mut next, client),

        Action::SendRequest { client, request } => apply_send_request(&mut next, client, request),

        Action::DeliverRequest {
            client,
            queue_index,
        } => apply_deliver_request(&mut next, client, queue_index),
    }

    next
}

/// Applies a login action: authenticates the client with given credentials.
fn apply_login(state: &mut SystemState, client: usize, user_id: u32, privileges: u64) {
    if let Some(session) = state.sessions.get_mut(client) {
        session.user_id = Some(user_id);
        session.privileges = privileges;
        state
            .effects
            .push(Effect::Authenticated { client, user_id });
    }
}

/// Applies a logout action: clears the client's authentication state.
fn apply_logout(state: &mut SystemState, client: usize) {
    if let Some(session) = state.sessions.get_mut(client) {
        session.user_id = None;
        session.privileges = 0;
        state.effects.push(Effect::LoggedOut { client });
    }
}

/// Applies a send request action: enqueues a message for later delivery.
fn apply_send_request(state: &mut SystemState, client: usize, request: RequestType) {
    if let Some(queue) = state.queues.get_mut(client) {
        queue.push(ModelMessage::new(request));
    }
}

/// Applies a deliver request action: processes a queued message.
///
/// This is where privilege enforcement occurs. The logic mirrors the mxd
/// server's `Session::require_privilege` and `Session::require_authenticated`.
fn apply_deliver_request(state: &mut SystemState, client: usize, queue_index: usize) {
    // Extract the message from the queue
    let message = match state.queues.get_mut(client) {
        Some(queue) if queue_index < queue.len() => queue.remove(queue_index),
        _ => return, // Invalid action, no state change
    };

    let request = message.request;
    let Some(session) = state.sessions.get(client) else {
        return; // Invalid client, no state change
    };

    // Check authentication requirement
    if request.requires_authentication() && !session.is_authenticated() {
        state
            .effects
            .push(Effect::RejectedUnauthenticated { client, request });
        return;
    }

    // Check privilege requirement
    let required = request.required_privilege();
    if required != 0 && !session.has_privilege(required) {
        state.effects.push(Effect::RejectedInsufficientPrivilege {
            client,
            request,
            required,
        });
        return;
    }

    // Request succeeded — record appropriate effect
    if request.is_privileged() {
        let session_privileges = session.privileges;
        state.effects.push(Effect::PrivilegedEffectCompleted {
            client,
            request,
            privilege: required,
            session_privileges,
        });
    } else {
        state
            .effects
            .push(Effect::UnprivilegedEffectCompleted { client, request });
    }
}

/// Returns `true` if the action is valid for the given state.
///
/// Used by the model to filter out actions that cannot be taken:
/// - `Login`: Client must not already be authenticated.
/// - `Logout`: Client must be authenticated.
/// - `SendRequest`: Always valid (for any client).
/// - `DeliverRequest`: Queue must contain a message at the given index.
#[must_use]
pub fn is_valid_action(state: &SystemState, action: &Action) -> bool {
    match *action {
        Action::Login { client, .. } => state
            .sessions
            .get(client)
            .is_some_and(|s| !s.is_authenticated()),
        Action::Logout { client } => state
            .sessions
            .get(client)
            .is_some_and(ModelSession::is_authenticated),
        Action::SendRequest { client, .. } => client < state.num_clients(),
        Action::DeliverRequest {
            client,
            queue_index,
        } => state
            .queues
            .get(client)
            .is_some_and(|q| queue_index < q.len()),
    }
}

#[cfg(test)]
mod tests;
