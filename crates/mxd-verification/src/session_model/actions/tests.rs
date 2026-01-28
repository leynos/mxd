//! Tests for session action transitions.

use rstest::rstest;

use super::*;
use crate::session_model::privileges::{DEFAULT_USER_PRIVILEGES, DOWNLOAD_FILE};

#[test]
fn login_authenticates_client() {
    let state = SystemState::new(2);
    let action = Action::Login {
        client: 0,
        user_id: 42,
        privileges: DEFAULT_USER_PRIVILEGES,
    };

    let next = apply_action(&state, &action);

    let session = next.sessions.first().expect("session exists");
    assert!(session.is_authenticated());
    assert_eq!(session.user_id, Some(42));
    assert_eq!(session.privileges, DEFAULT_USER_PRIVILEGES);
    assert!(matches!(
        next.effects.last(),
        Some(Effect::Authenticated {
            client: 0,
            user_id: 42
        })
    ));
}

#[test]
fn logout_clears_authentication() {
    let mut state = SystemState::new(2);
    let session = state.sessions.first_mut().expect("session exists");
    session.user_id = Some(42);
    session.privileges = DEFAULT_USER_PRIVILEGES;

    let action = Action::Logout { client: 0 };
    let next = apply_action(&state, &action);

    let next_session = next.sessions.first().expect("session exists");
    assert!(!next_session.is_authenticated());
    assert_eq!(next_session.privileges, 0);
    assert!(matches!(
        next.effects.last(),
        Some(Effect::LoggedOut { client: 0 })
    ));
}

#[test]
fn send_request_enqueues_message() {
    let state = SystemState::new(2);
    let action = Action::SendRequest {
        client: 0,
        request: RequestType::GetFileList,
    };

    let next = apply_action(&state, &action);

    let queue = next.queues.first().expect("queue exists");
    assert_eq!(queue.len(), 1);
    assert_eq!(
        queue.first().expect("message exists").request,
        RequestType::GetFileList
    );
}

#[derive(Clone, Copy, Debug)]
struct DeliverScenario {
    name: &'static str,
    user_id: Option<u32>,
    privileges: u64,
    request: RequestType,
    expected_effect: fn(Effect) -> bool,
}

#[rstest]
#[case(
    DeliverScenario {
        name: "deliver_request_rejects_unauthenticated",
        user_id: None,
        privileges: 0,
        request: RequestType::GetFileList,
        expected_effect: |effect| {
            matches!(effect, Effect::RejectedUnauthenticated { client: 0, .. })
        },
    }
)]
#[case(
    DeliverScenario {
        name: "deliver_request_rejects_insufficient_privilege",
        user_id: Some(42),
        privileges: 0,
        request: RequestType::GetFileList,
        expected_effect: |effect| {
            matches!(
                effect,
                Effect::RejectedInsufficientPrivilege {
                    client: 0,
                    required,
                    ..
                } if required == DOWNLOAD_FILE
            )
        },
    }
)]
#[case(
    DeliverScenario {
        name: "deliver_request_succeeds_with_privilege",
        user_id: Some(42),
        privileges: DOWNLOAD_FILE,
        request: RequestType::GetFileList,
        expected_effect: |effect| {
            matches!(
                effect,
                Effect::PrivilegedEffectCompleted {
                    client: 0,
                    privilege,
                    ..
                } if privilege == DOWNLOAD_FILE
            )
        },
    }
)]
#[case(
    DeliverScenario {
        name: "ping_succeeds_without_authentication",
        user_id: None,
        privileges: 0,
        request: RequestType::Ping,
        expected_effect: |effect| {
            matches!(effect, Effect::UnprivilegedEffectCompleted { client: 0, .. })
        },
    }
)]
fn deliver_request_scenarios(#[case] scenario: DeliverScenario) {
    let mut state = SystemState::new(2);
    if let Some(auth_user_id) = scenario.user_id {
        let session = state.sessions.first_mut().expect("session exists");
        session.user_id = Some(auth_user_id);
        session.privileges = scenario.privileges;
    }
    state
        .queues
        .first_mut()
        .expect("queue exists")
        .push(ModelMessage::new(scenario.request));

    let action = Action::DeliverRequest {
        client: 0,
        queue_index: 0,
    };
    let next = apply_action(&state, &action);

    assert!(
        next.queues.first().expect("queue exists").is_empty(),
        "case {}: message was not removed",
        scenario.name
    );
    let effect = *next.effects.last().expect("effect exists");
    assert!(
        (scenario.expected_effect)(effect),
        "case {}: unexpected effect {effect:?}",
        scenario.name
    );
}

#[test]
fn is_valid_action_checks_login_precondition() {
    let state = SystemState::new(2);
    let action = Action::Login {
        client: 0,
        user_id: 1,
        privileges: 0,
    };
    assert!(is_valid_action(&state, &action));

    // After login, another login should be invalid
    let next = apply_action(&state, &action);
    assert!(!is_valid_action(&next, &action));
}

#[test]
fn is_valid_action_checks_logout_precondition() {
    let state = SystemState::new(2);
    let action = Action::Logout { client: 0 };
    assert!(!is_valid_action(&state, &action)); // Not logged in

    // After login, logout should be valid
    let next = apply_action(
        &state,
        &Action::Login {
            client: 0,
            user_id: 1,
            privileges: 0,
        },
    );
    assert!(is_valid_action(&next, &action));
}

#[test]
fn is_valid_action_checks_deliver_precondition() {
    let state = SystemState::new(2);
    let action = Action::DeliverRequest {
        client: 0,
        queue_index: 0,
    };
    assert!(!is_valid_action(&state, &action)); // Empty queue

    let next = apply_action(
        &state,
        &Action::SendRequest {
            client: 0,
            request: RequestType::Ping,
        },
    );
    assert!(is_valid_action(&next, &action));
}
