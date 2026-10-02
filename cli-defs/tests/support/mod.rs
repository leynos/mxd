//! Child-process configuration fixtures following issue #509's environment boundary.

use std::process::Command;

use anyhow::{Context, Result, ensure};
use cap_std::fs_utf8::Dir;
use ortho_config::serde_json::{Value, json};
use tempfile::TempDir;

pub struct ConfigFixture {
    pub directory: Dir,
    temporary: TempDir,
}

impl ConfigFixture {
    pub fn new() -> Result<Self> {
        let temporary = TempDir::new()?;
        let path = temporary
            .path()
            .to_str()
            .context("temporary path must be UTF-8")?;
        let directory = Dir::open_ambient_dir(path, cap_std::ambient_authority())?;
        directory.create_dir("home")?;
        directory.create_dir("xdg")?;
        directory.create_dir("xdg-system")?;
        Ok(Self {
            directory,
            temporary,
        })
    }

    pub fn path(&self, name: &str) -> String {
        self.temporary
            .path()
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    pub fn probe(
        &self,
        args: &[&str],
        environment: &[(&str, &str)],
        expected: &Value,
    ) -> Result<()> {
        let output = Command::new(std::env::current_exe()?)
            .args(["--exact", "configuration_probe", "--nocapture"])
            .current_dir(self.temporary.path())
            .env_clear()
            .env("HOME", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("xdg"))
            .env("XDG_CONFIG_DIRS", self.path("xdg-system"))
            .env("MXD_TEST_ARGS", ortho_config::serde_json::to_string(args)?)
            .env("MXD_TEST_EXPECTED", expected.to_string())
            .envs(environment.iter().copied())
            .output()
            .context("run isolated configuration probe")?;
        ensure!(
            output.status.success(),
            "isolated configuration assertion failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }
}

pub fn defaults() -> Value {
    json!({
        "bind": "0.0.0.0:5500", "database": "mxd.db", "migration_timeout_secs": null,
        "argon2_m_cost": 19456, "argon2_t_cost": 2, "argon2_p_cost": 1
    })
}

pub fn file_values() -> Value {
    json!({
        "bind": "127.0.0.1:7100", "database": "file.db", "migration_timeout_secs": 7,
        "argon2_m_cost": 1024, "argon2_t_cost": 3, "argon2_p_cost": 2
    })
}

pub const ENVIRONMENT: &[(&str, &str)] = &[
    ("MXD_BIND", "127.0.0.1:7200"),
    ("MXD_DATABASE", "environment.db"),
    ("MXD_MIGRATION_TIMEOUT_SECS", "8"),
    ("MXD_ARGON2_M_COST", "2048"),
    ("MXD_ARGON2_T_COST", "4"),
    ("MXD_ARGON2_P_COST", "3"),
];

pub fn environment_values() -> Value {
    json!({
        "bind": "127.0.0.1:7200", "database": "environment.db", "migration_timeout_secs": 8,
        "argon2_m_cost": 2048, "argon2_t_cost": 4, "argon2_p_cost": 3
    })
}
