//! Capability-scoped filesystem helpers for staging and reading files.
//!
//! Tests stage files at paths they have just created (usually inside a
//! `TempDir`). These helpers open the file's parent directory as a
//! `cap_std::fs::Dir` handle and perform the operation through it, so callers
//! need no ambient `std::fs` calls. The ambient authority is requested here,
//! once, rather than scattered through the callers.
//!
//! # Permitted scope
//!
//! Use the helpers for fixture staging in tests, and for the validator's
//! configuration read, which takes an operator-supplied path
//! (`MXD_VALIDATOR_CONFIG`). [`read_to_string`] follows symbolic links the way
//! `std::fs::read_to_string` does, so a configuration file may be a link to a
//! file outside its directory. Do not use the helpers in the server's own
//! production code paths; those take explicit capability handles instead.

use std::{
    io,
    path::{Path, PathBuf},
};

use cap_std::{
    ambient_authority,
    fs::{Dir, Permissions, PermissionsExt as _},
};

/// The most symbolic links [`read_to_string`] follows before giving up.
const MAX_LINK_HOPS: usize = 40;

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

/// The nearest ancestor of `dir` that exists, treating an empty path as `.`.
fn nearest_existing_ancestor(dir: &Path) -> io::Result<PathBuf> {
    dir.ancestors()
        .map(|ancestor| {
            if ancestor.as_os_str().is_empty() {
                Path::new(".")
            } else {
                ancestor
            }
        })
        .find(|ancestor| ancestor.is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no existing ancestor of {}", dir.display()),
            )
        })
}

/// The part of `parent` below its nearest existing ancestor `existing`.
///
/// A relative `parent` whose ancestor is the working directory (`.`) is wholly
/// missing; `.` itself has nothing missing.
fn missing_suffix<'a>(parent: &'a Path, existing: &Path) -> io::Result<&'a Path> {
    if parent == Path::new(".") {
        Ok(Path::new(""))
    } else if existing == Path::new(".") && parent.is_relative() {
        Ok(parent)
    } else {
        parent
            .strip_prefix(existing)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

/// Write `contents` to the file at `path`, creating or truncating it.
///
/// # Errors
///
/// Returns an error if the parent directory cannot be opened or the file
/// cannot be written.
///
/// # Examples
///
/// ```
/// use test_util::fixture_fs::{read_to_string, write_file};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("note.txt");
/// write_file(&path, b"hello")?;
/// assert_eq!(
///     read_to_string(&path)?,
///     "hello",
///     "the file reads back as written"
/// );
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn write_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let (dir, name) = open_parent(path)?;
    dir.write(name, contents)
}

/// Write `contents` to the file at `path`, creating missing parent directories.
///
/// A relative `path` is resolved against the working directory.
///
/// # Errors
///
/// Returns an error if no ancestor of `path` exists, a directory cannot be
/// created, or the file cannot be written.
///
/// # Examples
///
/// ```
/// use test_util::fixture_fs::{read_to_string, write_file_creating_parents};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("a").join("b").join("note.txt");
/// write_file_creating_parents(&path, b"nested")?;
/// assert_eq!(read_to_string(&path)?, "nested", "the parents were created");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn write_file_creating_parents(path: &Path, contents: &[u8]) -> io::Result<()> {
    let (parent, _) = parent_and_name(path)?;
    let existing = nearest_existing_ancestor(&parent)?;
    let missing = missing_suffix(&parent, &existing)?;
    if !missing.as_os_str().is_empty() {
        Dir::open_ambient_dir(&existing, ambient_authority())?.create_dir_all(missing)?;
    }
    write_file(path, contents)
}

/// Write `contents` to the file at `path` and mark it executable (mode 0o755).
///
/// # Errors
///
/// Returns an error if the file cannot be written or its permissions cannot
/// be changed.
///
/// # Examples
///
/// ```
/// use std::os::unix::fs::PermissionsExt as _;
///
/// use test_util::fixture_fs::write_executable;
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("run.sh");
/// write_executable(&path, b"#!/bin/sh\n")?;
/// let mode = std::fs::metadata(&path)?.permissions().mode();
/// assert_eq!(mode & 0o777, 0o755, "the script is executable");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn write_executable(path: &Path, contents: &[u8]) -> io::Result<()> {
    let (dir, name) = open_parent(path)?;
    dir.write(&name, contents)?;
    dir.set_permissions(&name, Permissions::from_mode(0o755))
}

/// Read the whole file at `path` into a string, following symbolic links.
///
/// A link may point outside the directory that holds it: each hop re-opens the
/// directory that holds the link's target, up to a bounded number of hops.
///
/// # Errors
///
/// Returns an error if the file cannot be opened, is not valid UTF-8, or the
/// chain of links is too long.
///
/// # Examples
///
/// ```
/// use test_util::fixture_fs::{read_to_string, write_file};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("note.txt");
/// write_file(&path, b"hello")?;
/// assert_eq!(
///     read_to_string(&path)?,
///     "hello",
///     "the file reads back as written"
/// );
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn read_to_string(path: &Path) -> io::Result<String> {
    let mut current = path.to_path_buf();
    // Up to `MAX_LINK_HOPS` links are followed, then the final target is read.
    for _ in 0..=MAX_LINK_HOPS {
        let (dir, name) = open_parent(&current)?;
        // `read_link_contents` fails for anything that is not a symbolic link.
        let Ok(target) = dir.read_link_contents(&name) else {
            return dir.read_to_string(&name);
        };
        current = if target.is_absolute() {
            target
        } else {
            let (parent, _) = parent_and_name(&current)?;
            parent.join(target)
        };
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("too many symbolic links while reading {}", path.display()),
    ))
}

#[cfg(test)]
mod tests {
    //! Unit tests for the fixture filesystem helpers.

    use rstest::rstest;
    use tempfile::TempDir;

    use super::*;

    type Writer = fn(&Path, &[u8]) -> io::Result<()>;

    #[rstest]
    #[case::existing_directory(write_file, &["file.txt"])]
    #[case::missing_parents(write_file_creating_parents, &["a", "b", "file.txt"])]
    fn written_file_reads_back(#[case] writer: Writer, #[case] components: &[&str]) {
        let dir = TempDir::new().expect("temporary directory");
        let path = components
            .iter()
            .fold(dir.path().to_path_buf(), |acc, part| acc.join(part));

        writer(&path, b"hello").expect("write file");

        assert_eq!(
            read_to_string(&path).expect("read file"),
            "hello",
            "a written file should read back unchanged"
        );
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
        assert_eq!(mode & 0o777, 0o755, "the file should be mode 0755");
    }

    #[test]
    fn a_path_without_a_file_name_is_rejected() {
        let error = write_file(Path::new("/"), b"x").expect_err("root has no file name");

        assert_eq!(
            error.kind(),
            io::ErrorKind::InvalidInput,
            "a path with no file name is invalid input"
        );
    }

    #[test]
    fn relative_parents_missing_from_the_working_directory_are_created() {
        let dir = TempDir::new_in(".").expect("temporary directory in the working directory");
        let absolute = dir.path().join("new").join("sub").join("file.txt");
        let working_directory = std::env::current_dir().expect("working directory");
        let relative = absolute
            .strip_prefix(working_directory)
            .expect("the temporary directory is inside the working directory");

        write_file_creating_parents(relative, b"relative").expect("write relative file");

        assert_eq!(
            read_to_string(&absolute).expect("read file"),
            "relative",
            "a relative path with missing parents should be created in the working directory"
        );
    }

    #[test]
    fn a_link_to_a_file_outside_its_directory_is_followed() {
        let dir = TempDir::new().expect("temporary directory");
        let shared = dir.path().join("shared");
        let links = dir.path().join("links");
        write_file_creating_parents(&shared.join("real.toml"), b"value = 1").expect("write target");
        write_file_creating_parents(&links.join("keep"), b"").expect("create link directory");
        let absolute_link = links.join("absolute.toml");
        let relative_link = links.join("relative.toml");
        std::os::unix::fs::symlink(shared.join("real.toml"), &absolute_link)
            .expect("create absolute link");
        std::os::unix::fs::symlink("../shared/real.toml", &relative_link)
            .expect("create relative link");

        assert_eq!(
            read_to_string(&absolute_link).expect("read through absolute link"),
            "value = 1",
            "an absolute link to another directory should be followed"
        );
        assert_eq!(
            read_to_string(&relative_link).expect("read through relative link"),
            "value = 1",
            "a relative link that leaves its directory should be followed"
        );
    }

    #[rstest]
    #[case::at_the_limit(40, true)]
    #[case::beyond_the_limit(41, false)]
    fn link_chains_are_followed_up_to_the_limit(#[case] links: usize, #[case] readable: bool) {
        let dir = TempDir::new().expect("temporary directory");
        write_file(&dir.path().join("link-0"), b"end").expect("write target");
        for index in 1..=links {
            std::os::unix::fs::symlink(
                format!("link-{}", index - 1),
                dir.path().join(format!("link-{index}")),
            )
            .expect("create link");
        }

        let outcome = read_to_string(&dir.path().join(format!("link-{links}")));

        assert_eq!(
            outcome.is_ok(),
            readable,
            "a chain of {links} links should be readable: {readable}"
        );
    }

    #[test]
    fn a_missing_first_component_resolves_to_the_working_directory() {
        let existing = nearest_existing_ancestor(Path::new("no-such-top-level-dir/sub"))
            .expect("the working directory exists");

        assert_eq!(
            existing,
            Path::new("."),
            "a relative path with no existing component starts at the working directory"
        );
    }

    #[rstest]
    #[case::wholly_missing_relative("no-such-top-level-dir/sub", ".", "no-such-top-level-dir/sub")]
    #[case::working_directory_itself(".", ".", "")]
    #[case::below_an_existing_directory("/tmp/a/b", "/tmp", "a/b")]
    fn the_missing_suffix_is_what_lies_below_the_existing_ancestor(
        #[case] parent: &str,
        #[case] existing: &str,
        #[case] expected: &str,
    ) {
        let missing = missing_suffix(Path::new(parent), Path::new(existing))
            .expect("the existing ancestor is a prefix of the parent");

        assert_eq!(
            missing,
            Path::new(expected),
            "{parent} below {existing} should leave {expected} to create"
        );
    }

    #[test]
    fn a_link_loop_is_reported() {
        let dir = TempDir::new().expect("temporary directory");
        let looped = dir.path().join("loop");
        std::os::unix::fs::symlink("loop", &looped).expect("create self link");

        let error = read_to_string(&looped).expect_err("a link loop cannot be read");

        assert_eq!(
            error.kind(),
            io::ErrorKind::InvalidInput,
            "a link loop is reported as invalid input"
        );
    }
}
