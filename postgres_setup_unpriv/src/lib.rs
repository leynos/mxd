#![expect(
    non_snake_case,
    reason = "OrthoConfig derives emit helper modules with CamelCase identifiers"
)]
#![expect(
    missing_docs,
    reason = "OrthoConfig derive generates helper structs/functions without docs"
)]

//! Unprivileged `PostgreSQL` setup utilities.
//!
//! Provides helpers for staging `PostgreSQL` installations and data directories
//! using temporary privilege drops so the resulting assets are owned by an
//! unprivileged user.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::Path};

use color_eyre::eyre::{Context, Result, bail};
use nix::unistd::{Uid, chown, geteuid, getresuid, setresuid};
use ortho_config::OrthoConfig;
use postgresql_embedded::{PostgreSQL, Settings, VersionReq};
use serde::{Deserialize, Serialize};

/// Configuration for `PostgreSQL` environment settings loaded via `OrthoConfig`.
///
/// This struct provides configuration options for setting up a `PostgreSQL` instance,
/// including version constraints, connection parameters, and storage locations.
#[derive(Debug, Clone, Serialize, Deserialize, OrthoConfig, Default)]
#[ortho_config(prefix = "PG")]
pub struct PgEnvCfg {
    /// Version requirement specification, e.g. "=16.4.0" or "^17"
    pub version_req: Option<String>,
    /// Port number for the `PostgreSQL` server
    pub port: Option<u16>,
    /// Superuser name for the `PostgreSQL` instance
    pub superuser: Option<String>,
    /// Password for the superuser account
    pub password: Option<String>,
    /// Path to the data directory for the `PostgreSQL` instance
    pub data_dir: Option<std::path::PathBuf>,
    /// Path to the runtime/installation directory
    pub runtime_dir: Option<std::path::PathBuf>,
    /// Locale setting for the database
    pub locale: Option<String>,
    /// Character encoding for the database
    pub encoding: Option<String>,
}

impl PgEnvCfg {
    /// Converts the configuration into a complete `postgresql_embedded::Settings` object.
    ///
    /// Applies version, connection, path, and locale settings from the current configuration.
    /// Returns an error if the version requirement is invalid.
    ///
    /// # Errors
    ///
    /// Returns an error if the version requirement string cannot be parsed as a valid semver
    /// specification.
    pub fn to_settings(&self) -> Result<Settings> {
        let mut s = Settings::default();

        self.apply_version(&mut s)?;
        self.apply_connection(&mut s);
        self.apply_paths(&mut s);
        self.apply_locale(&mut s);

        Ok(s)
    }

    fn apply_version(&self, settings: &mut Settings) -> Result<()> {
        if let Some(ref vr) = self.version_req {
            settings.version =
                VersionReq::parse(vr).context("PG_VERSION_REQ invalid semver spec")?;
        }
        Ok(())
    }

    fn apply_connection(&self, settings: &mut Settings) {
        if let Some(p) = self.port {
            settings.port = p;
        }
        if let Some(ref u) = self.superuser {
            settings.username.clone_from(u);
        }
        if let Some(ref pw) = self.password {
            settings.password.clone_from(pw);
        }
    }

    fn apply_paths(&self, settings: &mut Settings) {
        if let Some(ref dir) = self.data_dir {
            settings.data_dir.clone_from(dir);
        }
        if let Some(ref dir) = self.runtime_dir {
            settings.installation_dir.clone_from(dir);
        }
    }

    /// Applies locale and encoding settings to the `PostgreSQL` configuration if specified
    /// in the environment.
    ///
    /// Inserts the `locale` and `encoding` values into the settings configuration map when
    /// present in the environment configuration.
    fn apply_locale(&self, settings: &mut Settings) {
        if let Some(ref loc) = self.locale {
            settings.configuration.insert("locale".into(), loc.clone());
        }
        if let Some(ref enc) = self.encoding {
            settings
                .configuration
                .insert("encoding".into(), enc.clone());
        }
    }
}

/// RAII guard that restores privileges on drop.
///
/// Used by [`with_temp_euid`] to ensure privileges are restored even if the
/// closure panics.
struct EuidGuard {
    real: Uid,
    saved: Uid,
}

impl Drop for EuidGuard {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "best-effort restore; Drop cannot propagate errors"
    )]
    fn drop(&mut self) { let _ = setresuid(self.real, Uid::from_raw(0), self.saved); }
}

/// Temporarily drop privileges to `target` UID for the duration of `body`.
///
/// # Safety constraints
///
/// This helper uses `setresuid`, which mutates **process-wide** privilege state
/// and is **not thread-safe**. Concurrent invocations from multiple threads will
/// race and can leave the process running under the wrong UID. Only call this
/// function from single-threaded contexts or protect it with external
/// synchronisation (for example, a process-wide mutex).
///
/// # Errors
///
/// Returns an error if the process is not running as root, if `setresuid`
/// fails, or if the provided closure returns an error.
///
/// # Implementation note
///
/// The guard's `Drop` implementation cannot propagate errors, so failures while
/// restoring privileges are silently discarded during the best-effort `setresuid`
/// call.
pub fn with_temp_euid<F, R>(target: Uid, body: F) -> Result<R>
where
    F: FnOnce() -> Result<R>,
{
    if !geteuid().is_root() {
        bail!("must start as root to drop privileges temporarily");
    }
    let ids = getresuid().context("getresuid failed")?;
    setresuid(ids.real, target, ids.saved).context("failed to set euid")?;

    let _guard = EuidGuard {
        real: ids.real,
        saved: ids.saved,
    };

    body()
}

/// Prepare `dir` so `uid` can access it.
///
/// Creates the directory if it does not exist, sets its owner to `uid`, and
/// applies permissions (0755) so the unprivileged user can read and execute its
/// contents.
///
/// # Errors
///
/// Returns an error if directory creation, ownership change, or permission
/// setting fails.
#[cfg(unix)]
pub fn make_dir_accessible<P: AsRef<Path>>(dir: P, uid: Uid) -> Result<()> {
    let path = dir.as_ref();
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    chown(path, Some(uid), None).with_context(|| format!("chown {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("chmod {}", path.display()))?;
    Ok(())
}

/// Returns the UID for the "nobody" user, or 65534 as fallback.
///
/// Looks up the "nobody" user in the system's user database. If the lookup
/// fails or the user doesn't exist, falls back to UID 65534 which is the
/// conventional "nobody" UID on most systems.
#[must_use]
pub fn nobody_uid() -> Uid {
    use nix::unistd::User;
    User::from_name("nobody")
        .ok()
        .flatten()
        .map_or_else(|| Uid::from_raw(65534), |u| u.uid)
}

/// Main entry point for the `PostgreSQL` unprivileged setup process.
///
/// Loads configuration via [`OrthoConfig`], sets up
/// directories with appropriate ownership, and initializes a `PostgreSQL`
/// instance using `postgresql_embedded`.
///
/// # Errors
///
/// Returns an error if:
/// - The process is not running as root
/// - Configuration loading fails
/// - Directory setup fails
/// - `PostgreSQL` setup fails
pub fn run() -> Result<()> {
    color_eyre::install()?;

    // Running as non-root can't change ownership of installation/data
    // directories. Fail fast instead of attempting setup and confusing users.
    if !geteuid().is_root() {
        bail!("must be run as root");
    }
    let cfg = PgEnvCfg::load().context("failed to load configuration via OrthoConfig")?;
    let settings = cfg.to_settings()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Tokio runtime")?;

    #[cfg(unix)]
    {
        let nobody = nobody_uid();
        make_dir_accessible(&settings.installation_dir, nobody)?;
        make_dir_accessible(&settings.data_dir, nobody)?;
        with_temp_euid(nobody, || {
            rt.block_on(async {
                let mut pg = PostgreSQL::new(settings);
                pg.setup()
                    .await
                    .wrap_err("postgresql_embedded::setup() failed")?;
                Ok::<(), color_eyre::Report>(())
            })
        })?;
    }
    #[cfg(not(unix))]
    {
        rt.block_on(async {
            let mut pg = PostgreSQL::new(settings);
            pg.setup()
                .await
                .wrap_err("postgresql_embedded::setup() failed")?;
            Ok::<(), color_eyre::Report>(())
        })?;
    }

    Ok(())
}
