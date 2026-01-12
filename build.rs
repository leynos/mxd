//! Build script for man page generation.
//!
//! Generates a man page for the `mxd` binary using `clap_mangen`. The CLI
//! definitions are imported from the `cli-defs` crate, which provides stable
//! types shared between build-time and runtime consumers.

use std::{env, fs, io, path::PathBuf};

use clap::CommandFactory;
use clap_mangen::Man;
use cli_defs::Cli;

fn main() -> io::Result<()> {
    println!("cargo::rerun-if-changed=cli-defs");

    let out_dir = match env::var("OUT_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => {
            // Cargo does not set OUT_DIR for `cargo check` or IDE analysis runs.
            return Ok(());
        }
    };
    let bin_name = env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "mxd".into());

    let cmd = Cli::command();
    let man = Man::new(cmd);

    let man_path = out_dir.join(format!("{bin_name}.1"));
    let mut file = fs::File::create(&man_path)?;
    man.render(&mut file)?;

    Ok(())
}
