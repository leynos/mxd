//! Build script for man page generation.
//!
//! Generates a man page for the `mxd` binary using `clap_mangen`. The CLI
//! module is included directly via path to reuse the same argument definitions
//! used by the runtime binary.

// The cli module triggers warnings when compiled in build script context
// because some types are not used and the OrthoConfig derive behaves
// differently. These are expected and safe to ignore here.
#![allow(dead_code, reason = "CLI types unused in build script context")]
#![allow(unused_imports, reason = "CLI module imports unused in build script")]
#![allow(
    unfulfilled_lint_expectations,
    reason = "OrthoConfig derive behaves differently in build script"
)]

use std::{env, fs, io, path::PathBuf};

use clap::CommandFactory;
use clap_mangen::Man;

#[path = "src/server/cli.rs"]
mod cli;

fn main() -> io::Result<()> {
    println!("cargo::rerun-if-changed=src/server/cli.rs");

    let out_dir = match env::var("OUT_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => return Ok(()),
    };
    let bin_name = env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "mxd".into());

    let cmd = cli::Cli::command();
    let man = Man::new(cmd);

    let man_path = out_dir.join(format!("{bin_name}.1"));
    let mut file = fs::File::create(&man_path)?;
    man.render(&mut file)?;

    Ok(())
}
