//! Login authentication strategy selection for wireframe routing.
//!
//! The guardrail router can route login command execution through strategy
//! implementations selected from compatibility metadata. For roadmap item 1.5.6
//! all strategies preserve current behaviour by delegating to
//! `Command::process_with_outbound`, but the abstraction keeps strategy choice
//! explicit for future SynHX/Hotline divergence.

use std::{future::Future, pin::Pin};

use crate::{
    commands::{Command, CommandContext, CommandError},
    wireframe::compat_policy::ClientKind,
};

/// Boxed future returned by [`AuthStrategy`] execution.
pub(crate) type AuthStrategyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), CommandError>> + Send + 'a>>;

/// Strategy abstraction for login authentication command execution.
pub(crate) trait AuthStrategy: Send + Sync {
    /// Execute login command processing for the selected strategy.
    fn authenticate<'a>(
        &self,
        command: Command,
        context: CommandContext<'a>,
    ) -> AuthStrategyFuture<'a>;
}

/// Default strategy for Hotline clients.
#[derive(Debug, Default)]
pub(crate) struct HotlineAuthStrategy;

/// Default strategy for `SynHX` clients.
#[derive(Debug, Default)]
pub(crate) struct SynHxAuthStrategy;

/// Default strategy for unknown clients.
#[derive(Debug, Default)]
pub(crate) struct UnknownAuthStrategy;

fn default_authenticate(command: Command, context: CommandContext<'_>) -> AuthStrategyFuture<'_> {
    Box::pin(async move { command.process_with_outbound(context).await })
}

impl AuthStrategy for HotlineAuthStrategy {
    fn authenticate<'a>(
        &self,
        command: Command,
        context: CommandContext<'a>,
    ) -> AuthStrategyFuture<'a> {
        default_authenticate(command, context)
    }
}

impl AuthStrategy for SynHxAuthStrategy {
    fn authenticate<'a>(
        &self,
        command: Command,
        context: CommandContext<'a>,
    ) -> AuthStrategyFuture<'a> {
        default_authenticate(command, context)
    }
}

impl AuthStrategy for UnknownAuthStrategy {
    fn authenticate<'a>(
        &self,
        command: Command,
        context: CommandContext<'a>,
    ) -> AuthStrategyFuture<'a> {
        default_authenticate(command, context)
    }
}

/// Return the selected auth strategy label for diagnostics and tests.
#[must_use]
pub(crate) const fn auth_strategy_label(client_kind: ClientKind) -> &'static str {
    match client_kind {
        ClientKind::Hotline85 | ClientKind::Hotline19 => "hotline-default",
        ClientKind::SynHx => "synhx-default",
        ClientKind::Unknown => "unknown-default",
    }
}

static HOTLINE_AUTH_STRATEGY: HotlineAuthStrategy = HotlineAuthStrategy;
static SYNHX_AUTH_STRATEGY: SynHxAuthStrategy = SynHxAuthStrategy;
static UNKNOWN_AUTH_STRATEGY: UnknownAuthStrategy = UnknownAuthStrategy;

/// Select the default login auth strategy for a classified client.
#[must_use]
pub(crate) fn auth_strategy_for_client(client_kind: ClientKind) -> &'static dyn AuthStrategy {
    match client_kind {
        ClientKind::Hotline85 | ClientKind::Hotline19 => &HOTLINE_AUTH_STRATEGY,
        ClientKind::SynHx => &SYNHX_AUTH_STRATEGY,
        ClientKind::Unknown => &UNKNOWN_AUTH_STRATEGY,
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::auth_strategy_label;
    use crate::wireframe::compat_policy::ClientKind;

    #[rstest]
    #[case(ClientKind::Hotline85, "hotline-default")]
    #[case(ClientKind::Hotline19, "hotline-default")]
    #[case(ClientKind::SynHx, "synhx-default")]
    #[case(ClientKind::Unknown, "unknown-default")]
    fn auth_strategy_label_matches_client_kind(
        #[case] client_kind: ClientKind,
        #[case] expected: &str,
    ) {
        assert_eq!(auth_strategy_label(client_kind), expected);
    }
}
