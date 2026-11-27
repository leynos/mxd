//! Version-floor tests for the `SQLite` capability audit.
//!
//! The floor has two distinct obligations, and they fail differently. The
//! *validator* has to accept exactly the triples at or above `(3, 35, 0)` for
//! an arbitrary input string. The *audit* has to actually call that validator
//! on the version the linked library reports, which no validator-level test can
//! observe: deleting the version step from the probe sequence would leave every
//! validator test passing.
//!
//! These tests are split from `tests.rs` by subject rather than by layer: all
//! three files are test-only and share the `db::audit` scope, and the split
//! keeps each module under the repository's 400-line ceiling.

use diesel_async::AsyncConnection;
use rstest::rstest;

use super::{Capability, audit_sqlite_features, ensure_supported_sqlite_version, test_seam};
use crate::db::DbConnection;

#[rstest]
#[case("3.35.0")]
#[case("3.45.1")]
// Deliberate parser-tolerance case: `sqlite_version()` returns a bare
// triple, and the build date belongs to `sqlite_source_id()`. Trailing
// text must still be ignored rather than treated as unparsable.
#[case("3.46.1 2024-08-13 09:16:08")]
fn version_accepts_supported(#[case] raw: &str) {
    let parsed = ensure_supported_sqlite_version(raw).expect("version must be accepted");
    assert!(parsed >= (3, 35, 0), "{raw} must meet the floor");
}

#[rstest]
#[case("3.34.1", "sqlite version 3.34.1 is not supported (require >= 3.35.0)")]
#[case("2.8.17", "sqlite version 2.8.17 is not supported (require >= 3.35.0)")]
#[case("", "unable to parse sqlite version: ")]
#[case("abc", "unable to parse sqlite version: abc")]
#[case("3.46", "unable to parse sqlite version: 3.46")]
fn version_rejects_unsupported(#[case] raw: &str, #[case] expected: &str) {
    let error =
        ensure_supported_sqlite_version(raw).expect_err("unsupported version must be rejected");
    assert!(
        error.to_string().contains(expected),
        "expected {error} to contain {expected}"
    );
}

/// The public audit rejects a library below the version floor.
///
/// `version_rejects_unsupported` calls the validator directly, and
/// `audit_reports_injected_probe_failure` aborts before the version fetch runs.
/// Neither can observe the version step surviving inside the probe sequence:
/// deleting the `ensure_supported_sqlite_version` call from the audit would
/// leave both — and every other test here — passing.
///
/// Here the version fetch reports a below-floor string instead of querying, so
/// only the step the public audit still has to call can reject it. The
/// capability named in the message also proves the failure was attributed to
/// the version probe rather than surfacing as a generic connection error.
#[rstest]
#[case("3.34.1")]
#[case("2.8.17")]
#[tokio::test]
async fn audit_rejects_a_version_below_the_floor(#[case] version: &str) {
    let mut conn = DbConnection::establish(":memory:")
        .await
        .expect("in-memory connection must open");

    let error = {
        let _injection = test_seam::inject_sqlite_version(version);
        audit_sqlite_features(&mut conn)
            .await
            .expect_err("an unsupported version must be rejected")
    };

    assert!(
        error.to_string().contains(Capability::Version.describe()),
        "the error must name the version capability: {error}"
    );
}

#[cfg(feature = "sqlite")]
mod version_contract {
    //! Acceptance is exactly the comparison the audit claims to make.
    //!
    //! The literal `rstest` cases elsewhere in this file pin a handful of
    //! strings, which cannot show that the parser agrees with the floor for an
    //! arbitrary triple. The contract is a range property over `u32` fields: a
    //! well-formed version string is accepted precisely when its triple is at
    //! least `(3, 35, 0)`. A sign error or a swapped `minor`/`patch` comparison
    //! would break it while leaving every literal case above passing.
    //!
    //! Both directions are asserted against the same generated triple, so
    //! neither "always accept" nor "always reject" can satisfy the property.
    //!
    //! The generator deliberately does not rely on `any::<u32>()` to stumble
    //! onto the boundary: a uniform draw almost never lands on `3.35.0`, and an
    //! off-by-one at the floor was measured to survive `any::<u32>()` alone.
    //! The `near_floor` strategy pins `major`, `minor` and the patch values
    //! around the boundary, and `any::<u32>()` covers the wide range.

    use proptest::prelude::*;

    use super::ensure_supported_sqlite_version;

    const FLOOR: (u32, u32, u32) = (3, 35, 0);

    /// Triples clustered on the comparison boundary, where a `<` versus `<=`
    /// mistake is observable.
    fn near_floor() -> impl Strategy<Value = (u32, u32, u32)> {
        prop_oneof![
            Just(FLOOR),
            Just((3, 34, u32::MAX)),
            Just((3, 35, 1)),
            Just((3, 36, 0)),
            Just((2, u32::MAX, u32::MAX)),
            (
                0u32..3,
                0u32..40,
                prop_oneof![Just(0u32), Just(1), Just(34), Just(35)]
            ),
        ]
    }

    fn any_triple() -> impl Strategy<Value = (u32, u32, u32)> {
        prop_oneof![near_floor(), (any::<u32>(), any::<u32>(), any::<u32>())]
    }

    proptest! {
        #[test]
        fn acceptance_matches_the_floor((major, minor, patch) in any_triple()) {
            let raw = format!("{major}.{minor}.{patch}");
            let accepted = ensure_supported_sqlite_version(&raw).is_ok();
            prop_assert_eq!(accepted, (major, minor, patch) >= FLOOR, "triple {}", raw);
        }
    }

    /// The exact floor is the one input where `>=` and `>` differ.
    ///
    /// Kept as a named test so a failing control points at a readable
    /// assertion rather than at a shrinking strategy.
    #[test]
    fn the_floor_is_accepted() {
        assert!(ensure_supported_sqlite_version("3.35.0").is_ok());
        assert!(ensure_supported_sqlite_version("3.34.65535").is_err());
        assert!(ensure_supported_sqlite_version("2.65535.65535").is_err());
    }
}
