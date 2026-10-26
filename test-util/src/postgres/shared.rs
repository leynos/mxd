//! Shared cluster initialization with a fallback for upstream slot races.

use std::{error::Error, sync::OnceLock};

use pg_embedded_setup_unpriv::{
    ClusterHandle,
    test_support::{ensure_worker_env, shared_cluster_handle as upstream_shared_cluster_handle},
};

use super::embedded::{
    EmbeddedPgError,
    describe_error_chain,
    is_slot_contention,
    start_cluster_retrying_slot_contention,
};

static FALLBACK_CLUSTER: OnceLock<Result<ClusterHandle, String>> = OnceLock::new();

/// Use the upstream singleton unless its cached initialization lost a slot race.
pub(super) fn shared_cluster_handle() -> Result<&'static ClusterHandle, EmbeddedPgError> {
    resolve_shared_handle(
        &FALLBACK_CLUSTER,
        upstream_shared_cluster_handle,
        initialize_fallback_cluster,
    )
}

/// Preserve the upstream failure policy except for the slot race it cannot retry.
fn resolve_shared_handle<'a, T, E: Error>(
    fallback: &'a OnceLock<Result<T, String>>,
    primary: impl FnOnce() -> Result<&'a T, E>,
    initialize: impl FnOnce() -> Result<T, EmbeddedPgError>,
) -> Result<&'a T, EmbeddedPgError> {
    match primary() {
        Ok(handle) => Ok(handle),
        Err(error) if is_slot_contention(&error) => fallback
            .get_or_init(|| initialize().map_err(|failure| failure.to_string()))
            .as_ref()
            .map_err(|message| EmbeddedPgError::BootstrapFailed(message.clone())),
        Err(error) => Err(EmbeddedPgError::BootstrapFailed(format!(
            "bootstrapping shared embedded PostgreSQL: {}",
            describe_error_chain(&error)
        ))),
    }
}

/// Keep the fallback alive until the upstream shutdown hook runs at exit.
fn initialize_fallback_cluster() -> Result<ClusterHandle, EmbeddedPgError> {
    let worker_guard = ensure_worker_env();
    let (handle, guard) = start_cluster_retrying_slot_contention()?;
    let guarded = guard.with_worker_guard(worker_guard);
    handle.register_shutdown_on_exit().map_err(|error| {
        EmbeddedPgError::BootstrapFailed(format!(
            "registering shared PostgreSQL shutdown: {}",
            describe_error_chain(&error)
        ))
    })?;
    std::mem::forget(guarded);
    Ok(handle)
}

#[cfg(test)]
mod tests {
    //! Exercise fallback selection, retries, caching and error reporting.

    use std::{cell::Cell, future::ready, io, sync::OnceLock};

    use futures_util::FutureExt;

    use super::resolve_shared_handle;
    use crate::postgres::embedded::{EmbeddedPgError, retry_slot_contention};

    fn slot_failure() -> io::Error { io::Error::other("slot lock /test is already held") }

    /// Inject one lost slot before invoking the selected bootstrap.
    fn fail_first<T>(
        attempts: &Cell<u32>,
        start: impl FnOnce() -> Result<T, io::Error>,
    ) -> Result<T, io::Error> {
        attempts.set(attempts.get() + 1);
        if attempts.get() == 1 {
            Err(slot_failure())
        } else {
            start()
        }
    }

    /// Run a synchronous test bootstrap through the production retry policy.
    fn retry_test_start<T>(
        mut start: impl FnMut() -> Result<T, io::Error>,
    ) -> Result<T, EmbeddedPgError> {
        retry_slot_contention(|| ready(start()), |_| ready(()))
            .now_or_never()
            .ok_or_else(|| EmbeddedPgError::BootstrapFailed("test retry suspended".into()))?
    }

    fn start_real_cluster() -> Result<
        (
            pg_embedded_setup_unpriv::ClusterHandle,
            pg_embedded_setup_unpriv::ClusterGuard,
        ),
        io::Error,
    > {
        super::start_cluster_retrying_slot_contention().map_err(io::Error::other)
    }

    #[test]
    fn slot_failure_retries_fallback_before_caching_template_handle() {
        let fallback = OnceLock::new();
        let attempts = Cell::new(0);
        let initializes = Cell::new(0);
        let resolve = || {
            resolve_shared_handle(
                &fallback,
                || Err(slot_failure()),
                || {
                    initializes.set(initializes.get() + 1);
                    retry_test_start(|| fail_first(&attempts, || Ok("template-ready")))
                },
            )
        };

        let handle = resolve().expect("fallback succeeds after contention");
        assert_eq!(*handle, "template-ready");
        assert!(std::ptr::eq(handle, resolve().expect("cached handle")));
        assert_eq!(attempts.get(), 2);
        assert_eq!(initializes.get(), 1);
    }

    #[test]
    fn slot_race_fallback_supports_template_database() -> anyhow::Result<()> {
        let fallback = OnceLock::new();
        let attempts = Cell::new(0);
        let (handle, _guard) = resolve_shared_handle(
            &fallback,
            || Err(slot_failure()),
            || retry_test_start(|| fail_first(&attempts, start_real_cluster)),
        )?;
        handle.ensure_template_exists("fallback_test_template", |_| Ok(()))?;
        handle.create_database_from_template("fallback_test_database", "fallback_test_template")?;
        anyhow::ensure!(
            handle.database_exists("fallback_test_database")?,
            "the database cloned from the template was not found"
        );
        anyhow::ensure!(attempts.get() == 2, "expected exactly two startup attempts");
        Ok(())
    }

    #[test]
    fn terminal_primary_error_reports_its_cause_without_fallback() {
        let fallback: OnceLock<Result<(), String>> = OnceLock::new();
        let result = resolve_shared_handle(
            &fallback,
            || {
                Err(io::Error::other(io::Error::other(
                    "underlying server failure",
                )))
            },
            || {
                Err(EmbeddedPgError::BootstrapFailed(
                    "unexpected fallback".into(),
                ))
            },
        );

        let error = result.expect_err("unrelated startup error remains terminal");
        assert!(error.to_string().contains("underlying server failure"));
        assert!(fallback.get().is_none());
    }

    #[test]
    fn exhausted_fallback_is_cached_with_its_cause() {
        let fallback: OnceLock<Result<(), String>> = OnceLock::new();
        let initializes = Cell::new(0);
        let resolve = || {
            resolve_shared_handle(
                &fallback,
                || Err(slot_failure()),
                || {
                    initializes.set(initializes.get() + 1);
                    Err(EmbeddedPgError::BootstrapFailed(
                        "last slot lock cause".into(),
                    ))
                },
            )
        };

        for _ in 0..2 {
            assert!(
                resolve()
                    .expect_err("failure is cached")
                    .to_string()
                    .contains("last slot lock cause")
            );
        }
        assert_eq!(initializes.get(), 1);
    }
}
