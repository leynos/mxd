//! Build script for man page generation.
//!
//! Generates a man page for the `mxd` binary using `clap_mangen`. The CLI
//! definitions are imported from the `cli-defs` crate, which provides stable
//! types shared between build-time and runtime consumers.

use std::{env, io};

use camino::{Utf8Path, Utf8PathBuf};
use cap_std::fs_utf8::Dir;
use clap::CommandFactory;
use clap_mangen::Man;
use cli_defs::Cli;

fn main() -> io::Result<()> {
    println!("cargo::rerun-if-changed=cli-defs");

    let out_dir_path = match env::var("OUT_DIR") {
        Ok(dir) => Utf8PathBuf::from(dir),
        Err(_) => {
            // Cargo does not set OUT_DIR for `cargo check` or IDE analysis runs.
            return Ok(());
        }
    };
    let out_dir = Dir::open_ambient_dir(&out_dir_path, cap_std::ambient_authority())?;
    let bin_name = env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "mxd".into());

    let cmd = Cli::command();
    let man = Man::new(cmd);

    let man_file = format!("{bin_name}.1");
    let man_path = Utf8Path::new(&man_file);
    let mut file = out_dir.create(man_path)?;
    man.render(&mut file)?;

    Ok(())
}
