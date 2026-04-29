//! Tests for migration timeout configuration and watchdog cancellation.

use std::time::Duration;

use diesel::result::Error as DieselError;
use rstest::rstest;
use tokio_util::sync::CancellationToken;

use super::{DEFAULT_MIGRATION_TIMEOUT, migration_timeout, run_with_migration_timeout};

#[rstest]
#[case(None, DEFAULT_MIGRATION_TIMEOUT)]
#[case(Some(0), DEFAULT_MIGRATION_TIMEOUT)]
#[case(Some(7), Duration::from_secs(7))]
fn migration_timeout_maps_configuration_to_duration(
    #[case] input: Option<u64>,
    #[case] expected: Duration,
) {
    assert_eq!(migration_timeout(input), expected);
}

#[tokio::test]
async fn migration_watchdog_allows_work_that_finishes_in_time() {
    let result =
        run_with_migration_timeout(Duration::from_secs(1), CancellationToken::new(), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<(), DieselError>(())
        })
        .await;

    let inner = result.expect("watchdog should not trip for completed work");
    assert_eq!(inner.expect("inner migration work should succeed"), ());
}

#[tokio::test]
async fn migration_watchdog_cancels_work_and_reports_the_applied_timeout() {
    let err =
        run_with_migration_timeout(Duration::from_millis(1), CancellationToken::new(), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<(), DieselError>(())
        })
        .await
        .expect_err("cancelled work should time out");

    let DieselError::SerializationError(inner) = err else {
        panic!("timeout should be wrapped as a serialization error");
    };

    assert_eq!(inner.to_string(), "migration execution exceeded 1ms");
}
