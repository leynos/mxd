//! Helpers for `PostgreSQL`-backed integration tests.
//!
//! Every database here is created in an embedded cluster started through
//! `pg-embed-setup-unpriv`. There is deliberately no route to an external
//! server: a test that could run against whatever a developer or runner
//! happened to provide would pass or skip for reasons nobody decided.

pub(crate) mod common;
mod embedded;

pub use common::{DatabaseName, DatabaseNameError, DatabaseUrl, PostgresTestDbError};
use common::reset_postgres_db;
use embedded::{
    EmbeddedPg,
    EmbeddedPgError,
    start_embedded_postgres,
    start_embedded_postgres_async,
    start_embedded_postgres_with_strategy,
};

/// A test database in an embedded `PostgreSQL` cluster.
///
/// Dropping it drops the database, and stops the cluster when this database
/// owns one.
pub struct PostgresTestDb {
    /// The connection URL for the test database.
    pub url: DatabaseUrl,
    _embedded: EmbeddedPg,
}

impl PostgresTestDb {
    /// Map an embedded-cluster failure onto the public error type.
    fn map_embedded_err(error: EmbeddedPgError) -> PostgresTestDbError {
        match error {
            EmbeddedPgError::BootstrapFailed(message) => {
                PostgresTestDbError::EmbeddedBootstrapFailed(message)
            }
            EmbeddedPgError::InitFailed(inner) => {
                PostgresTestDbError::EmbeddedInitFailed(inner.to_string())
            }
        }
    }

    /// Wrap a started embedded database.
    fn from_embedded(embedded: EmbeddedPg) -> Self {
        Self {
            url: embedded.url.clone(),
            _embedded: embedded,
        }
    }

    /// Creates a test database in a freshly started embedded cluster.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresTestDbError::EmbeddedBootstrapFailed`] when the
    /// cluster cannot be bootstrapped or started, and
    /// [`PostgresTestDbError::EmbeddedInitFailed`] when the test database
    /// cannot be created or prepared.
    pub fn new() -> Result<Self, PostgresTestDbError> {
        start_embedded_postgres(reset_postgres_db)
            .map(Self::from_embedded)
            .map_err(Self::map_embedded_err)
    }

    /// Creates a test database from an async context, on the caller's runtime.
    ///
    /// # Errors
    ///
    /// As for [`Self::new`].
    pub async fn new_async() -> Result<Self, PostgresTestDbError> {
        start_embedded_postgres_async(reset_postgres_db)
            .await
            .map(Self::from_embedded)
            .map_err(Self::map_embedded_err)
    }

    /// Creates a test database by cloning a migrated template.
    ///
    /// The template is created once per process in the shared cluster, with
    /// migrations applied, and each later database is cloned from it, which
    /// takes tens of milliseconds rather than seconds.
    ///
    /// # Errors
    ///
    /// As for [`Self::new`].
    pub fn new_from_template() -> Result<Self, PostgresTestDbError> {
        start_embedded_postgres_with_strategy(reset_postgres_db, true)
            .map(Self::from_embedded)
            .map_err(Self::map_embedded_err)
    }
}

pub use fixture_glue::{postgres_db, postgres_db_fast};

mod fixture_glue {
    //! `rstest` fixture glue for `PostgreSQL` test databases.

    #![expect(
        unused_braces,
        reason = "rstest #[fixture] macro expands from normal function bodies"
    )]

    use rstest::fixture;

    use super::{PostgresTestDb, PostgresTestDbError};

    /// rstest fixture providing a `PostgreSQL` test database.
    ///
    /// This fixture is for tests that require `PostgreSQL` and should report
    /// setup errors through the test runner.
    #[fixture]
    pub fn postgres_db() -> Result<PostgresTestDb, PostgresTestDbError> { PostgresTestDb::new() }

    /// rstest fixture providing a fast `PostgreSQL` test database via template
    /// cloning.
    #[fixture]
    pub fn postgres_db_fast() -> Result<PostgresTestDb, PostgresTestDbError> {
        PostgresTestDb::new_from_template()
    }
}

#[cfg(test)]
mod tests {
    //! Error mapping from the embedded cluster onto the public error type.

    use std::io;

    use super::{EmbeddedPgError, PostgresTestDb, PostgresTestDbError};

    #[test]
    fn embedded_error_mapping_preserves_variants() {
        let bootstrap = PostgresTestDb::map_embedded_err(EmbeddedPgError::BootstrapFailed(
            "missing".to_owned(),
        ));
        assert!(
            matches!(bootstrap, PostgresTestDbError::EmbeddedBootstrapFailed(_)),
            "a cluster that cannot start should map to EmbeddedBootstrapFailed"
        );

        let init_failed = PostgresTestDb::map_embedded_err(EmbeddedPgError::InitFailed(Box::new(
            io::Error::other("boom"),
        )));
        assert!(
            matches!(init_failed, PostgresTestDbError::EmbeddedInitFailed(_)),
            "embedded initialization failures should map to EmbeddedInitFailed"
        );
    }
}
