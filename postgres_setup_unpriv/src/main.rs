//! Bootstraps a PostgreSQL data directory as the `nobody` user.
//!
//! Configuration is provided via environment variables parsed by
//! [`OrthoConfig`](https://github.com/leynos/ortho-config). The binary exits
//! with status code `0` on success and `1` on error.

use std::process::exit;

fn main() {
    if let Err(e) = postgres_setup_unpriv::run() {
        eprintln!("postgres-setup-unpriv: {e:#}");
        exit(1);
    }
}
