//! Capability-scoped filesystem helpers for staging test fixtures.
//!
//! Tests stage files at paths they have just created (usually inside a
//! `TempDir`). These helpers open the file's parent directory as a
//! `cap_std::fs::Dir` handle and perform the operation through it, so test
//! code needs no ambient `std::fs` calls. The ambient authority is requested
//! here, once, rather than scattered through the tests.

use std::{
    io,
    path::{Path, PathBuf},
};

use cap_std::{
    ambient_authority,
    fs::{Dir, Permissions, PermissionsExt as _},
};

/// Split `path` into the directory that holds it and its file name.
fn parent_and_name(path: &Path) -> io::Result<(PathBuf, PathBuf)> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no file name", path.display()),
        )
    })?;
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Ok((parent, PathBuf::from(name)))
}

/// Open the directory that holds `path` and return it with the file name.
fn open_parent(path: &Path) -> io::Result<(Dir, PathBuf)> {
    let (parent, name) = parent_and_name(path)?;
    Ok((Dir::open_ambient_dir(parent, ambient_authority())?, name))
}

/// Write `contents` to the file at `path`, creating or truncating it.
///
/// # Errors
///
/// Returns an error if the parent directory cannot be opened or the file
/// cannot be written.
pub fn write_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let (dir, name) = open_parent(path)?;
    dir.write(name, contents)
}

/// Write `contents` to the file at `path`, creating missing parent directories.
///
/// # Errors
///
/// Returns an error if no ancestor of `path` exists, a directory cannot be
/// created, or the file cannot be written.
pub fn write_file_creating_parents(path: &Path, contents: &[u8]) -> io::Result<()> {
    let (parent, _) = parent_and_name(path)?;
    let existing = parent
        .ancestors()
        .find(|ancestor| ancestor.is_dir())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no existing ancestor of {}", parent.display()),
            )
        })?;
    let missing = parent
        .strip_prefix(existing)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    if !missing.as_os_str().is_empty() {
        Dir::open_ambient_dir(existing, ambient_authority())?.create_dir_all(missing)?;
    }
    write_file(path, contents)
}

/// Write `contents` to the file at `path` and mark it executable (mode 0o755).
///
/// # Errors
///
/// Returns an error if the file cannot be written or its permissions cannot
/// be changed.
pub fn write_executable(path: &Path, contents: &[u8]) -> io::Result<()> {
    let (dir, name) = open_parent(path)?;
    dir.write(&name, contents)?;
    dir.set_permissions(&name, Permissions::from_mode(0o755))
}

/// Read the whole file at `path` into a string.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or is not valid UTF-8.
pub fn read_to_string(path: &Path) -> io::Result<String> {
    let (dir, name) = open_parent(path)?;
    dir.read_to_string(name)
}

#[cfg(test)]
mod tests {
    //! Unit tests for the fixture filesystem helpers.

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn written_file_reads_back() {
        let dir = TempDir::new().expect("temporary directory");
        let path = dir.path().join("file.txt");

        write_file(&path, b"hello").expect("write file");

        assert_eq!(read_to_string(&path).expect("read file"), "hello");
    }

    #[test]
    fn parents_are_created_on_request() {
        let dir = TempDir::new().expect("temporary directory");
        let path = dir.path().join("a").join("b").join("file.txt");

        write_file_creating_parents(&path, b"nested").expect("write file");

        assert_eq!(read_to_string(&path).expect("read file"), "nested");
    }

    #[test]
    fn executables_carry_the_execute_bits() {
        let dir = TempDir::new().expect("temporary directory");
        let path = dir.path().join("script");

        write_executable(&path, b"#!/bin/sh\n").expect("write executable");

        let (parent, name) = open_parent(&path).expect("open parent");
        let mode = parent
            .metadata(name)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn a_path_without_a_file_name_is_rejected() {
        let error = write_file(Path::new("/"), b"x").expect_err("root has no file name");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
