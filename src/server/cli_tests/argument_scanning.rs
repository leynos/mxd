//! Property tests for the subcommand-boundary scan.
//!
//! `config_args_from` decides which tokens reach the parser `OrthoConfig` adds, so
//! a misclassified token silently changes the configuration the server loads.
//! The outcome depends on token order and on which prefixes clap accepts, which
//! makes it a property over generated argument sequences rather than a fixed
//! list of examples. The process-level contracts in `tests/cli_contracts`
//! remain the end-to-end specification; these properties exercise the scan
//! itself.

use std::ffi::OsString;

use proptest::prelude::*;

use super::*;

/// Global options, paired with a value that clap accepts for each.
const GLOBAL_OPTIONS: [(&str, &str); 6] = [
    ("--bind", "127.0.0.1:5500"),
    ("--database", "mxd.db"),
    ("--migration-timeout-secs", "30"),
    ("--argon2-m-cost", "19456"),
    ("--argon2-t-cost", "2"),
    ("--argon2-p-cost", "1"),
];

/// Every option index, used where repetition would invalidate the parse.
const ALL_OPTIONS: [usize; 6] = [0, 1, 2, 3, 4, 5];

/// Option indices other than `--database`, which the retention cases reuse.
const WITHOUT_DATABASE: [usize; 5] = [0, 2, 3, 4, 5];

/// The subcommand name the scan must recognise.
const CREATE_USER: &str = "create-user";

/// Build an argument vector whose first element is the program name.
///
/// # Examples
///
/// ```text
/// argv([OsString::from("--database")]) == ["mxd", "--database"]
/// ```
fn argv(parts: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    std::iter::once(OsString::from("mxd"))
        .chain(parts)
        .collect()
}

/// The two tokens for one option and its value.
fn option_tokens(index: usize) -> [OsString; 2] {
    let (flag, value) = GLOBAL_OPTIONS[index];
    [OsString::from(flag), OsString::from(value)]
}

/// Concatenated tokens for a sequence of options, each given once.
fn option_tokens_for(indices: &[usize]) -> Vec<OsString> {
    indices
        .iter()
        .flat_map(|&index| option_tokens(index))
        .collect()
}

/// Tokens for a `--database` option whose value spells the command name.
fn database_command_value() -> [OsString; 2] {
    [OsString::from("--database"), OsString::from(CREATE_USER)]
}

proptest! {
    /// While `--database` still waits for its value, clap rejects the prefix,
    /// so the command name stays a value instead of becoming a boundary.
    #[test]
    fn command_name_value_is_not_a_boundary(
        before in prop::sample::subsequence(&ALL_OPTIONS[..], 0..=6),
        after in prop::sample::subsequence(&ALL_OPTIONS[..], 0..=2),
    ) {
        let mut parts = option_tokens_for(&before);
        parts.extend(database_command_value());
        parts.extend(option_tokens_for(&after));
        let args = argv(parts);

        prop_assert_eq!(config_args_from(&args), args.clone());
    }

    /// An actual subcommand and its arguments are removed from the
    /// configuration arguments, whatever valid options precede it.
    #[test]
    fn actual_subcommand_is_stripped(
        options in prop::sample::subsequence(&ALL_OPTIONS[..], 0..=6),
    ) {
        let prefix = argv(option_tokens_for(&options));
        let mut args = prefix.clone();
        args.extend([
            OsString::from(CREATE_USER),
            OsString::from("alice"),
            OsString::from("secret"),
        ]);

        prop_assert_eq!(config_args_from(&args), prefix);
    }

    /// The `--` separator ends classification, so a command name after it
    /// remains a literal argument.
    #[test]
    fn separator_ends_the_scan(
        options in prop::sample::subsequence(&ALL_OPTIONS[..], 0..=6),
    ) {
        let mut args = argv(option_tokens_for(&options));
        args.extend([OsString::from("--"), OsString::from(CREATE_USER)]);

        prop_assert_eq!(config_args_from(&args), args.clone());
    }

    /// A command-name option value survives when a real subcommand follows,
    /// so the value and the boundary are told apart in one vector.
    #[test]
    fn command_name_value_survives_before_a_real_subcommand(
        options in prop::sample::subsequence(&WITHOUT_DATABASE[..], 0..=5),
    ) {
        let mut prefix = argv(option_tokens_for(&options));
        prefix.extend(database_command_value());
        let mut args = prefix.clone();
        args.extend([OsString::from(CREATE_USER), OsString::from("alice")]);

        prop_assert_eq!(config_args_from(&args), prefix);
    }
}

/// A subcommand as the first token leaves only the program name.
#[test]
fn bare_subcommand_leaves_the_program_name() {
    let args = argv([
        OsString::from(CREATE_USER),
        OsString::from("alice"),
        OsString::from("secret"),
    ]);

    assert_eq!(
        config_args_from(&args),
        vec![OsString::from("mxd")],
        "the subcommand and its arguments are stripped",
    );
}

/// An option repeated with the command name as its value keeps every token,
/// so the duplicate-option error is reported instead of silently dropping
/// arguments that follow the value.
#[test]
fn repeated_option_keeps_command_name_value() {
    let args = argv([
        OsString::from("--database"),
        OsString::from("first.db"),
        OsString::from("--database"),
        OsString::from(CREATE_USER),
    ]);

    assert_eq!(
        config_args_from(&args),
        args.clone(),
        "no prefix without a value for the second --database parses",
    );
}
