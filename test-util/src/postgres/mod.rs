//! Helpers for `PostgreSQL`-backed integration tests.

pub(crate) mod common;
mod embedded;

pub use common::{
    DatabaseName,
    DatabaseNameError,
    DatabaseUrl,
    PostgresTestDbError,
    PostgresUnavailable,
};
use common::{
    create_external_db_if_available,
    drop_external_db,
    postgres_test_url_from_env,
    reset_postgres_db,
};
use embedded::{
    EmbeddedPg,
    EmbeddedPgError,
    start_embedded_postgres,
    start_embedded_postgres_async,
    start_embedded_postgres_with_strategy,
};

/// A test database instance backed by either embedded or external `PostgreSQL`.
pub struct PostgresTestDb {
    /// The connection URL for the test database.
    pub url: DatabaseUrl,
    admin_url: Option<DatabaseUrl>,
    embedded: Option<EmbeddedPg>,
    db_name: Option<DatabaseName>,
}

impl PostgresTestDb {
    fn admin_url_from_env() -> Result<Option<DatabaseUrl>, PostgresTestDbError> {
        postgres_test_url_from_env()
            .map(|value| DatabaseUrl::parse(&value).map_err(PostgresTestDbError::UrlParse))
            .transpose()
    }

    fn map_embedded_err(error: EmbeddedPgError) -> PostgresTestDbError {
        match error {
            EmbeddedPgError::Unavailable(_) => {
                PostgresTestDbError::Unavailable(PostgresUnavailable)
            }
            EmbeddedPgError::InitFailed(inner) => {
                PostgresTestDbError::EmbeddedInitFailed(inner.to_string())
            }
        }
    }

    /// Creates a new test database instance.
    ///
    /// Uses `POSTGRES_TEST_URL` environment variable if set, otherwise starts
    /// an embedded `PostgreSQL` instance.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresTestDbError::Unavailable`] if:
    /// - The configured external server cannot be reached
    /// - The embedded `PostgreSQL` binary is not available
    ///
    /// Returns semantic [`PostgresTestDbError`] variants for other errors
    /// (URL parsing, database creation, etc.).
    pub fn new() -> Result<Self, PostgresTestDbError> {
        if let Some(admin_url) = Self::admin_url_from_env()? {
            let (url, db_name) = create_external_db_if_available(&admin_url)?;
            return Ok(Self {
                url,
                admin_url: Some(admin_url),
                embedded: None,
                db_name: Some(db_name),
            });
        }

        let embedded =
            start_embedded_postgres(reset_postgres_db).map_err(Self::map_embedded_err)?;
        let url = embedded.url.clone();
        let db_name = embedded.db_name.clone();
        Ok(Self {
            url,
            admin_url: None,
            embedded: Some(embedded),
            db_name: Some(db_name),
        })
    }

    /// Creates a new test database instance from an async context.
    ///
    /// Uses `POSTGRES_TEST_URL` environment variable if set, otherwise starts
    /// an embedded `PostgreSQL` instance using the caller runtime.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresTestDbError::Unavailable`] if:
    /// - The configured external server cannot be reached
    /// - The embedded `PostgreSQL` binary is not available
    ///
    /// Returns semantic [`PostgresTestDbError`] variants for other errors
    /// (URL parsing, database creation, etc.).
    pub async fn new_async() -> Result<Self, PostgresTestDbError> {
        if let Some(admin_url) = Self::admin_url_from_env()? {
            let admin_url_for_blocking = admin_url.clone();
            let (url, db_name) = tokio::task::spawn_blocking(move || {
                create_external_db_if_available(&admin_url_for_blocking)
            })
            .await
            .map_err(PostgresTestDbError::BlockingTaskFailed)??;
            return Ok(Self {
                url,
                admin_url: Some(admin_url),
                embedded: None,
                db_name: Some(db_name),
            });
        }

        let embedded = start_embedded_postgres_async(reset_postgres_db)
            .await
            .map_err(Self::map_embedded_err)?;
        let url = embedded.url.clone();
        let db_name = embedded.db_name.clone();
        Ok(Self {
            url,
            admin_url: None,
            embedded: Some(embedded),
            db_name: Some(db_name),
        })
    }

    /// Creates a new test database instance using template-based cloning.
    ///
    /// This method provides significantly faster database creation by:
    /// - Creating a template database once (with migrations applied)
    /// - Cloning subsequent databases from the template (10-50ms vs 2-10s)
    ///
    /// Like [`Self::new`], uses `POSTGRES_TEST_URL` if set, otherwise embedded `PostgreSQL`.
    ///
    /// # Performance
    ///
    /// - First test: ~5-10 seconds (template creation + migration)
    /// - Subsequent tests: ~10-50ms (template clone only)
    ///
    /// # Errors
    ///
    /// Returns [`PostgresTestDbError::Unavailable`] if `PostgreSQL` is unreachable.
    /// Returns semantic [`PostgresTestDbError`] variants for initialization errors.
    pub fn new_from_template() -> Result<Self, PostgresTestDbError> {
        if std::env::var_os("POSTGRES_TEST_URL").is_some() {
            // External PostgreSQL: template support not yet implemented for external servers
            // Fall back to regular database creation
            return Self::new();
        }

        let embedded = start_embedded_postgres_with_strategy(reset_postgres_db, true).map_err(
            |e| match e {
                EmbeddedPgError::Unavailable(_) => {
                    PostgresTestDbError::Unavailable(PostgresUnavailable)
                }
                EmbeddedPgError::InitFailed(inner) => {
                    PostgresTestDbError::EmbeddedInitFailed(inner.to_string())
                }
            },
        )?;
        let url = embedded.url.clone();
        let db_name = embedded.db_name.clone();
        Ok(Self {
            url,
            admin_url: None,
            embedded: Some(embedded),
            db_name: Some(db_name),
        })
    }

    /// Returns `true` if this test database uses an embedded `PostgreSQL` instance.
    #[must_use]
    pub const fn uses_embedded(&self) -> bool { self.embedded.is_some() }
}

impl Drop for PostgresTestDb {
    #[expect(
        clippy::print_stderr,
        reason = "best-effort cleanup; Drop cannot propagate errors"
    )]
    fn drop(&mut self) {
        if let Some(embedded) = self.embedded.take() {
            drop(embedded);
            return;
        }
        if let (Some(admin), Some(name)) = (&self.admin_url, &self.db_name) {
            drop_external_db(admin, name);
        } else {
            let url = self.url.clone();
            let result = std::thread::spawn(move || reset_postgres_db(&url)).join();
            match result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    eprintln!("reset_postgres_db cleanup failed: {err}");
                }
                Err(err) => {
                    eprintln!("reset_postgres_db cleanup thread panicked: {err:?}");
                }
            }
        }
    }
}

pub use fixture_glue::{postgres_db, postgres_db_fast};

mod fixture_glue {
    //! `rstest` fixture glue for `PostgreSQL` test databases.

    #![expect(
        missing_docs,
        reason = "rstest #[fixture] macro generates undocumented helper items"
    )]
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
    //! Regression tests for external `PostgreSQL` URL probing.

    use std::io;

    use anyhow::{bail, ensure};
    use rstest::rstest;
    use url::Url;

    use super::{EmbeddedPgError, PostgresTestDb, PostgresTestDbError, common::probe_port};
    use crate::{AnyError, with_env_var};

    #[rstest]
    #[case("postgres://postgres:password@127.0.0.1/test", Some(5432))]
    #[case("postgresql://postgres:password@127.0.0.1/test", Some(5432))]
    #[case("postgres://postgres:password@127.0.0.1:6543/test", Some(6543))]
    fn probe_port_matches_postgres_url_policy(#[case] url: &str, #[case] expected: Option<u16>) {
        let parsed = Url::parse(url).expect("test URL should parse");
        assert_eq!(probe_port(&parsed), expected);
    }

    #[test]
    fn admin_url_from_env_returns_none_when_unset() -> Result<(), AnyError> {
        with_env_var("POSTGRES_TEST_URL", None, || {
            ensure!(
                PostgresTestDb::admin_url_from_env()?.is_none(),
                "POSTGRES_TEST_URL should be unset"
            );
            Ok(())
        })
    }

    #[test]
    fn admin_url_from_env_maps_parse_errors_to_url_parse() -> Result<(), AnyError> {
        with_env_var("POSTGRES_TEST_URL", Some("not a postgres url"), || {
            let Err(error) = PostgresTestDb::admin_url_from_env() else {
                bail!("invalid POSTGRES_TEST_URL should fail");
            };
            ensure!(
                matches!(error, PostgresTestDbError::UrlParse(_)),
                "invalid POSTGRES_TEST_URL should map to UrlParse"
            );
            Ok(())
        })
    }

    #[test]
    fn new_maps_env_parse_errors_to_url_parse() -> Result<(), AnyError> {
        with_env_var("POSTGRES_TEST_URL", Some("not a postgres url"), || {
            let Err(error) = PostgresTestDb::new() else {
                bail!("invalid POSTGRES_TEST_URL should fail");
            };
            ensure!(
                matches!(error, PostgresTestDbError::UrlParse(_)),
                "invalid POSTGRES_TEST_URL should map to UrlParse"
            );
            Ok(())
        })
    }

    #[test]
    fn new_async_maps_env_parse_errors_to_url_parse() -> Result<(), AnyError> {
        with_env_var("POSTGRES_TEST_URL", Some("not a postgres url"), || {
            let runtime = tokio::runtime::Builder::new_current_thread().build()?;
            let Err(error) = runtime.block_on(PostgresTestDb::new_async()) else {
                bail!("invalid POSTGRES_TEST_URL should fail");
            };
            ensure!(
                matches!(error, PostgresTestDbError::UrlParse(_)),
                "invalid POSTGRES_TEST_URL should map to UrlParse"
            );
            Ok(())
        })
    }

    #[test]
    fn embedded_error_mapping_preserves_variants() {
        let unavailable =
            PostgresTestDb::map_embedded_err(EmbeddedPgError::Unavailable("missing".to_owned()));
        assert!(
            matches!(unavailable, PostgresTestDbError::Unavailable(_)),
            "embedded unavailability should map to Unavailable"
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
