// Library for postgres_setup_unpriv

use anyhow::{bail, Context, Result};
use nix::unistd::{getresuid, geteuid, setresuid, Uid};
use ortho_config::OrthoConfig;
use postgresql_embedded::{settings::AuthMethod, PostgreSQL, Settings, VersionReq};
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
    pub auth_method: Option<String>,
}

impl PgEnvCfg {
    /// Convert into a fully‑formed `postgresql_embedded::Settings`.
    pub fn to_settings(&self) -> Result<Settings> {
        let mut s = Settings::default();

        if let Some(ref vr) = self.version_req {
            s.version = VersionReq::parse(vr).context("PG_VERSION_REQ invalid semver spec")?;
        }
        if let Some(p) = self.port {
            s.port = p;
        }
        if let Some(ref u) = self.superuser {
            s.username = u.clone();
        }
        if let Some(ref pw) = self.password {
            s.password = pw.clone();
        }
        if let Some(ref dir) = self.data_dir {
            s.data_dir = dir.clone();
        }
        if let Some(ref dir) = self.runtime_dir {
            s.installation_dir = dir.clone();
        }
        if let Some(ref loc) = self.locale {
            s.configuration.insert("locale".into(), loc.clone());
        }
        if let Some(ref enc) = self.encoding {
            s.configuration.insert("encoding".into(), enc.clone());
        }
        if let Some(ref am) = self.auth_method {
            s.auth_method = match am.to_ascii_lowercase().as_str() {
                "trust" => AuthMethod::Trust,
                "password" => AuthMethod::Password,
                other => bail!("unknown PG_AUTH_METHOD '{other}'"),
            };
        }

        Ok(s)
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

pub fn run() -> Result<()> {
    let cfg = PgEnvCfg::load().context("failed to load configuration via OrthoConfig")?;
    let settings = cfg.to_settings()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Tokio runtime")?;

    with_temp_euid(Uid::from_raw(65534), || {
        rt.block_on(async {
            let mut pg = PostgreSQL::new(settings);
            pg.setup().await.context("postgresql_embedded::setup() failed")?;
            Ok::<(), anyhow::Error>(())
        })
    })?;

    Ok(())
}

