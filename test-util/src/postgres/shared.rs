//! Shared cluster initialization with a fallback for upstream slot races.

use std::{cell::Cell, error::Error, sync::OnceLock};

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

/// Initialize the shared cluster, falling back if the upstream singleton lost a slot race.
///
/// For example, repeated calls return the same initialized handle.
#[tracing::instrument(skip_all, fields(operation = "shared_cluster_initialize"))]
pub(super) fn initialize_shared_cluster() -> Result<&'static ClusterHandle, EmbeddedPgError> {
    initialize_or_select_shared_cluster(
        &FALLBACK_CLUSTER,
        upstream_shared_cluster_handle,
        initialize_fallback_cluster,
    )
}

/// Initialize or reuse a fallback after a primary slot-contention failure.
///
/// For example, an unrelated primary failure is returned without a fallback.
fn initialize_or_select_shared_cluster<'a, T, E: Error>(
    fallback: &'a OnceLock<Result<T, String>>,
    primary: impl FnOnce() -> Result<&'a T, E>,
    initialize: impl FnOnce() -> Result<T, EmbeddedPgError>,
) -> Result<&'a T, EmbeddedPgError> {
    match primary() {
        Ok(handle) => {
            tracing::debug!(
                source = "primary",
                outcome = "success",
                "shared cluster selected"
            );
            Ok(handle)
        }
        Err(error) if is_slot_contention(&error) => initialize_fallback(fallback, initialize),
        Err(error) => Err(EmbeddedPgError::BootstrapFailed(format!(
            "bootstrapping shared embedded PostgreSQL: {}",
            describe_error_chain(&error)
        ))),
    }
}

/// Initialize or reuse the fallback, recording its bounded cache outcome.
/// For example, a second call reuses the result of the first initialization.
fn initialize_fallback<T>(
    fallback: &OnceLock<Result<T, String>>,
    initialize: impl FnOnce() -> Result<T, EmbeddedPgError>,
) -> Result<&T, EmbeddedPgError> {
    metrics::counter!("mxd_test_postgres_fallback_selected_total").increment(1);
    let initialized = Cell::new(false);
    let cached = fallback.get_or_init(|| {
        initialized.set(true);
        initialize().map_err(|failure| failure.to_string())
    });
    let outcome = if cached.is_ok() { "success" } else { "failure" };
    let cache_state = if initialized.get() {
        "initialized"
    } else {
        "reused"
    };
    metrics::counter!("mxd_test_postgres_fallback_cache_total", "outcome" => outcome, "state" => cache_state).increment(1);
    tracing::warn!(
        source = "fallback",
        error_category = "slot_contention",
        cache_state,
        outcome,
        "shared cluster fallback selected"
    );
    cached
        .as_ref()
        .map_err(|message| EmbeddedPgError::BootstrapFailed(message.clone()))
}

/// Keep the fallback alive until the upstream shutdown hook runs at exit.
/// For example, successful registration returns a process-lifetime handle.
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

    use std::{
        cell::{Cell, RefCell},
        future::ready,
        sync::OnceLock,
        time::Duration,
    };

    use futures_util::FutureExt;

    use super::initialize_or_select_shared_cluster;
    use crate::postgres::{
        embedded::{EmbeddedPgError, retry_slot_contention},
        test_errors::NestedError,
    };

    /// Construct a contention error whose outer display hides its cause.
    fn slot_failure() -> NestedError { NestedError::new("slot lock /test is already held") }

    /// Inject one lost slot before invoking the selected bootstrap.
    fn fail_first<T>(
        attempts: &Cell<u32>,
        start: impl FnOnce() -> Result<T, NestedError>,
    ) -> Result<T, NestedError> {
        attempts.set(attempts.get() + 1);
        if attempts.get() == 1 {
            Err(slot_failure())
        } else {
            start()
        }
    }

    /// Run a synchronous test bootstrap through the production retry policy.
    fn retry_test_start<T>(
        mut start: impl FnMut() -> Result<T, NestedError>,
    ) -> Result<T, EmbeddedPgError> {
        retry_slot_contention(|| ready(start()), |_| ready(()))
            .now_or_never()
            .ok_or_else(|| EmbeddedPgError::BootstrapFailed("test retry suspended".into()))?
    }

    /// Start a real cluster for the template fallback regression.
    fn start_real_cluster() -> Result<
        (
            pg_embedded_setup_unpriv::ClusterHandle,
            pg_embedded_setup_unpriv::ClusterGuard,
        ),
        NestedError,
    > {
        super::start_cluster_retrying_slot_contention()
            .map_err(|error| NestedError::new(&error.to_string()))
    }

    /// Verify contention retries precede caching the shared fallback handle.
    #[test]
    fn slot_failure_retries_fallback_before_caching_template_handle() {
        let fallback = OnceLock::new();
        let attempts = Cell::new(0);
        let initializes = Cell::new(0);
        let resolve = || {
            initialize_or_select_shared_cluster(
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

    /// Verify a retried fallback supports template creation and cloning.
    #[test]
    fn slot_race_fallback_supports_template_database() -> anyhow::Result<()> {
        let fallback = OnceLock::new();
        let attempts = Cell::new(0);
        let (handle, _guard) = initialize_or_select_shared_cluster(
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

    /// Verify non-contention primary failures retain their cause without fallback.
    #[test]
    fn terminal_primary_error_reports_its_cause_without_fallback() {
        let fallback: OnceLock<Result<(), String>> = OnceLock::new();
        let result = initialize_or_select_shared_cluster(
            &fallback,
            || Err(NestedError::new("primary terminal source marker")),
            || {
                Err(EmbeddedPgError::BootstrapFailed(
                    "unexpected fallback".into(),
                ))
            },
        );

        let error = result.expect_err("unrelated startup error remains terminal");
        assert!(error.to_string().contains("primary terminal source marker"));
        assert!(fallback.get().is_none());
    }

    /// Simulate a lost slot followed by a different terminal failure.
    fn terminal_fallback_start(
        attempts: &Cell<u32>,
        delays: &RefCell<Vec<Duration>>,
    ) -> Result<(), EmbeddedPgError> {
        retry_slot_contention(
            || {
                attempts.set(attempts.get() + 1);
                let cause = if attempts.get() == 1 {
                    "slot lock /test is already held"
                } else {
                    "fallback terminal source marker"
                };
                ready(Err::<(), _>(NestedError::new(cause)))
            },
            |delay| {
                delays.borrow_mut().push(delay);
                ready(())
            },
        )
        .now_or_never()
        .ok_or_else(|| EmbeddedPgError::BootstrapFailed("test retry suspended".into()))?
    }

    /// Verify a terminal fallback cause survives caching and repeated lookup.
    #[test]
    fn terminal_fallback_failure_is_cached_with_its_cause() {
        let fallback: OnceLock<Result<(), String>> = OnceLock::new();
        let initializes = Cell::new(0);
        let attempts = Cell::new(0);
        let delays = RefCell::new(Vec::new());
        let resolve = || {
            initialize_or_select_shared_cluster(
                &fallback,
                || Err(slot_failure()),
                || {
                    initializes.set(initializes.get() + 1);
                    terminal_fallback_start(&attempts, &delays)
                },
            )
        };

        for _ in 0..2 {
            assert!(
                resolve()
                    .expect_err("failure is cached")
                    .to_string()
                    .contains("fallback terminal source marker")
            );
        }
        assert_eq!(initializes.get(), 1);
        assert_eq!(attempts.get(), 2);
        assert_eq!(*delays.borrow(), vec![Duration::from_millis(25)]);
    }
}
