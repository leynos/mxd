// Library for postgres_setup_unpriv
#![allow(non_snake_case)]

use anyhow::{bail, Context, Result};
use nix::unistd::{getresuid, geteuid, setresuid, Uid};
use ortho_config::OrthoConfig;
use postgresql_embedded::{PostgreSQL, Settings, VersionReq};
use serde::{Deserialize, Serialize};

#[allow(non_snake_case)]
#[derive(Debug, Clone, Serialize, Deserialize, OrthoConfig, Default)]
#[ortho_config(prefix = "PG")]
pub struct PgEnvCfg {
    /// e.g. "=16.4.0" or "^17"
    pub version_req: Option<String>,
    pub port: Option<u16>,
    pub superuser: Option<String>,
    pub password: Option<String>,
    pub data_dir: Option<std::path::PathBuf>,
    pub runtime_dir: Option<std::path::PathBuf>,
    pub locale: Option<String>,
    pub encoding: Option<String>,
}

impl PgEnvCfg {
    /// Converts the configuration into a complete `postgresql_embedded::Settings` object.
    ///
    /// Applies version, connection, path, and locale settings from the current configuration.
    /// Returns an error if the version requirement is invalid.
    ///
    /// # Returns
    /// A fully configured `Settings` instance on success, or an error if configuration fails.
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
            settings.username = u.clone();
        }
        if let Some(ref pw) = self.password {
            settings.password = pw.clone();
        }
    }

    fn apply_paths(&self, settings: &mut Settings) {
        if let Some(ref dir) = self.data_dir {
            settings.data_dir = dir.clone();
        }
        if let Some(ref dir) = self.runtime_dir {
            settings.installation_dir = dir.clone();
        }
    }

    /// Applies locale and encoding settings to the PostgreSQL configuration if specified in the environment.
    ///
    /// Inserts the `locale` and `encoding` values into the settings configuration map when present in the environment configuration.
    fn apply_locale(&self, settings: &mut Settings) {
        if let Some(ref loc) = self.locale {
            settings.configuration.insert("locale".into(), loc.clone());
        }
        if let Some(ref enc) = self.encoding {
            settings.configuration.insert("encoding".into(), enc.clone());
        }
    }

}

/// Temporary privilege drop helper (process‑wide!)
pub fn with_temp_euid<F, R>(target: Uid, body: F) -> Result<R>
where
    F: FnOnce() -> Result<R>,
{
    if !geteuid().is_root() {
        bail!("must start as root to drop privileges temporarily");
    }
    let ids = getresuid().context("getresuid failed")?;
    setresuid(ids.real, target, ids.saved).context("failed to set euid")?;

    struct Guard {
        real: Uid,
        saved: Uid,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = setresuid(self.real, Uid::from_raw(0), self.saved);
        }
    }

    let _guard = Guard {
        real: ids.real,
        saved: ids.saved,
    };

    body()
}

pub fn nobody_uid() -> Uid {
    use nix::unistd::User;
    User::from_name("nobody")
        .ok()
        .flatten()
        .map(|u| u.uid)
        .unwrap_or_else(|| Uid::from_raw(65534))
}

pub fn run() -> Result<()> {
    let cfg = PgEnvCfg::load().context("failed to load configuration via OrthoConfig")?;
    let settings = cfg.to_settings()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Tokio runtime")?;

    with_temp_euid(nobody_uid(), || {
        rt.block_on(async {
            let mut pg = PostgreSQL::new(settings);
            pg.setup().await.context("postgresql_embedded::setup() failed")?;
            Ok::<(), anyhow::Error>(())
        })
    })?;

    Ok(())
}

