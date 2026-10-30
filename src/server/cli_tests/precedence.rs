//! Conflicting configuration layers for the server CLI.

use super::*;

struct ExpectedConfiguration {
    database: &'static str,
    migration_timeout: Option<u64>,
    time_cost: u32,
    parallelism: u32,
}

#[rstest]
#[case::defaults(
    vec!["mxd"],
    false,
    false,
    ExpectedConfiguration {
        database: "mxd.db",
        migration_timeout: None,
        time_cost: DEFAULT_ARGON2_T_COST,
        parallelism: DEFAULT_ARGON2_P_COST,
    }
)]
#[case::file(
    vec!["mxd"],
    true,
    false,
    ExpectedConfiguration {
        database: "file.db",
        migration_timeout: Some(31),
        time_cost: 3,
        parallelism: 4,
    }
)]
#[case::environment(
    vec!["mxd"],
    true,
    true,
    ExpectedConfiguration {
        database: "env.db",
        migration_timeout: Some(32),
        time_cost: 5,
        parallelism: 6,
    }
)]
#[case::command_line(
    vec![
        "mxd", "--database", "cli.db", "--migration-timeout-secs", "33",
        "--argon2-t-cost", "7", "--argon2-p-cost", "8"
    ],
    true,
    true,
    ExpectedConfiguration {
        database: "cli.db",
        migration_timeout: Some(33),
        time_cost: 7,
        parallelism: 8,
    }
)]
fn conflicting_layers_resolve_each_field_by_precedence(
    #[case] args: Vec<&str>,
    #[case] has_file_values: bool,
    #[case] has_environment_values: bool,
    #[case] expected: ExpectedConfiguration,
) -> Result<()> {
    let root = TempDir::new().context("create isolated config root")?;
    let mut environment = isolated_environment(&root)?;
    if has_file_values {
        let path = write_config(
            &root,
            "tiers.toml",
            concat!(
                "database = 'file.db'\n",
                "migration_timeout_secs = 31\n",
                "argon2_t_cost = 3\n",
                "argon2_p_cost = 4\n"
            ),
        )?;
        environment = select_file(environment, &path);
    }
    if has_environment_values {
        environment = environment
            .with_var("MXD_DATABASE", "env.db")
            .with_var("MXD_MIGRATION_TIMEOUT_SECS", "32")
            .with_var("MXD_ARGON2_T_COST", "5")
            .with_var("MXD_ARGON2_P_COST", "6");
    }

    let cfg = load_config(args, environment)?;
    ensure_equal!(
        cfg.database.as_str(),
        expected.database,
        "database precedence"
    )?;
    ensure_equal!(
        cfg.migration_timeout_secs,
        expected.migration_timeout,
        "timeout precedence"
    )?;
    ensure_equal!(
        cfg.argon2_t_cost,
        expected.time_cost,
        "Argon2 time precedence"
    )?;
    ensure_equal!(
        cfg.argon2_p_cost,
        expected.parallelism,
        "Argon2 parallelism precedence"
    )?;
    Ok(())
}
