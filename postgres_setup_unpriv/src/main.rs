//! Bootstraps a PostgreSQL data directory as the `nobody` user.
//!
//! Configuration is provided via environment variables parsed by
//! [`OrthoConfig`](https://github.com/leynos/ortho-config). The binary exits
//! with status code `0` on success and `1` on error.

use anyhow::{bail, Context, Result};
use nix::unistd::{geteuid, setresuid, Uid};
use ortho_config::OrthoConfig;
use postgresql_embedded::{settings::AuthMethod, PostgreSQL, Settings, VersionReq};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::exit};

#[derive(Debug, Clone, Serialize, Deserialize, OrthoConfig)]
#[ortho_config(prefix = "PG")]
struct PgEnvCfg {
    version_req: Option<String>,
    port: Option<u16>,
    superuser: Option<String>,
    password: Option<String>,
    data_dir: Option<PathBuf>,
    runtime_dir: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    locale: Option<String>,
    encoding: Option<String>,
    auth_method: Option<String>,
}

impl PgEnvCfg {
    fn to_settings(&self) -> Result<Settings> {
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
        if let Some(ref dir) = self.cache_dir {
            s.installation_dir = dir.clone();
        }
        if let Some(ref loc) = self.locale {
            s.configuration.insert("locale".into(), loc.clone());
        }
        if let Some(ref enc) = self.encoding {
            s.configuration.insert("encoding".into(), enc.clone());
        }
        if let Some(ref am) = self.auth_method {
            s.configuration.insert(
                "auth_method".into(),
                match am.to_ascii_lowercase().as_str() {
                    "trust" => "trust".to_string(),
                    "password" => "password".to_string(),
                    other => bail!("unknown PG_AUTH_METHOD '{other}'"),
                },
            );
        }

        Ok(s)
    }
}

fn with_temp_euid<F, R>(target: Uid, body: F) -> Result<R>
where
    F: FnOnce() -> Result<R>,
{
    if !geteuid().is_root() {
        bail!("must start as root to drop privileges temporarily");
    }

    setresuid(None, Some(target), None).context("failed to set euid")?;

    let res = body();

    setresuid(None, Some(Uid::from_raw(0)), None).context("failed to restore euid")?;

    res
}

fn main() {
    if let Err(e) = run() {
        eprintln!("postgres-setup-unpriv: {e:#}");
        exit(1);
    }
}

fn run() -> Result<()> {
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
