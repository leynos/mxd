//! Command-line interface definitions for the MXD server.
//!
//! This module re-exports CLI types from the `cli-defs` crate, which provides
//! stable definitions shared between build-time (man page generation) and
//! runtime consumers.

pub use cli_defs::{
    AppConfig,
    Cli,
    Commands,
    CreateUserArgs,
    DEFAULT_ARGON2_M_COST,
    DEFAULT_ARGON2_P_COST,
    DEFAULT_ARGON2_T_COST,
};

#[cfg(test)]
mod tests {
    use argon2::Params;
    use figment::Jail;
    use rstest::rstest;

    use super::*;

    /// Verifies that our local constants match the upstream argon2 crate defaults.
    ///
    /// This guards against silent drift if argon2 changes its defaults in a
    /// future release.
    #[rstest]
    fn argon2_default_constants_match_upstream() {
        assert_eq!(
            DEFAULT_ARGON2_M_COST,
            Params::DEFAULT_M_COST,
            "DEFAULT_ARGON2_M_COST should match argon2::Params::DEFAULT_M_COST"
        );
        assert_eq!(
            DEFAULT_ARGON2_T_COST,
            Params::DEFAULT_T_COST,
            "DEFAULT_ARGON2_T_COST should match argon2::Params::DEFAULT_T_COST"
        );
        assert_eq!(
            DEFAULT_ARGON2_P_COST,
            Params::DEFAULT_P_COST,
            "DEFAULT_ARGON2_P_COST should match argon2::Params::DEFAULT_P_COST"
        );
    }

    /// Verifies `AppConfig` uses the expected Argon2 defaults when no overrides
    /// are provided.
    #[rstest]
    fn argon2_defaults_applied_to_config() {
        Jail::expect_with(|_j| {
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.argon2_m_cost, DEFAULT_ARGON2_M_COST);
            assert_eq!(cfg.argon2_t_cost, DEFAULT_ARGON2_T_COST);
            assert_eq!(cfg.argon2_p_cost, DEFAULT_ARGON2_P_COST);
            Ok(())
        });
    }

    #[rstest]
    fn env_config_loading() {
        Jail::expect_with(|j| {
            j.set_env("MXD_BIND", "127.0.0.1:8000");
            j.set_env("MXD_DATABASE", "env.db");
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.bind, "127.0.0.1:8000");
            assert_eq!(cfg.database, "env.db".to_string());
            Ok(())
        });
    }

    #[rstest]
    fn cli_overrides_env() {
        Jail::expect_with(|j| {
            j.set_env("MXD_BIND", "127.0.0.1:8000");
            let cfg = AppConfig::load_from_iter(["mxd", "--bind", "0.0.0.0:9000"]).expect("load");
            assert_eq!(cfg.bind, "0.0.0.0:9000");
            Ok(())
        });
    }

    #[rstest]
    fn loads_from_dotfile() {
        Jail::expect_with(|j| {
            j.create_file(".mxd.toml", "bind = \"1.2.3.4:1111\"")?;
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.bind, "1.2.3.4:1111".to_string());
            Ok(())
        });
    }

    #[rstest]
    fn argon2_cli_overrides_all_params() {
        Jail::expect_with(|_j| {
            let cfg = AppConfig::load_from_iter([
                "mxd",
                "--argon2-m-cost",
                "1024",
                "--argon2-t-cost",
                "4",
                "--argon2-p-cost",
                "2",
            ])
            .expect("load");
            assert_eq!(cfg.argon2_m_cost, 1024);
            assert_eq!(cfg.argon2_t_cost, 4);
            assert_eq!(cfg.argon2_p_cost, 2);
            Ok(())
        });
    }

    #[rstest]
    fn argon2_env_overrides() {
        Jail::expect_with(|j| {
            j.set_env("MXD_ARGON2_M_COST", "2048");
            j.set_env("MXD_ARGON2_T_COST", "8");
            j.set_env("MXD_ARGON2_P_COST", "4");
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.argon2_m_cost, 2048);
            assert_eq!(cfg.argon2_t_cost, 8);
            assert_eq!(cfg.argon2_p_cost, 4);
            Ok(())
        });
    }

    #[rstest]
    fn argon2_config_file_overrides() {
        Jail::expect_with(|j| {
            j.create_file(
                ".mxd.toml",
                concat!(
                    "argon2_m_cost = 4096\n",
                    "argon2_t_cost = 16\n",
                    "argon2_p_cost = 8\n"
                ),
            )?;
            let cfg = AppConfig::load_from_iter(["mxd"]).expect("load");
            assert_eq!(cfg.argon2_m_cost, 4096);
            assert_eq!(cfg.argon2_t_cost, 16);
            assert_eq!(cfg.argon2_p_cost, 8);
            Ok(())
        });
    }

    #[rstest]
    fn argon2_cli_overrides_env_and_file() {
        Jail::expect_with(|j| {
            j.create_file(".mxd.toml", "argon2_m_cost = 4096\n")?;
            j.set_env("MXD_ARGON2_M_COST", "2048");
            let cfg = AppConfig::load_from_iter(["mxd", "--argon2-m-cost", "1024"]).expect("load");
            assert_eq!(cfg.argon2_m_cost, 1024);
            Ok(())
        });
    }
}
