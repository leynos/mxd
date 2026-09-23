//! What the `CodeScene` uploader may no longer be asked for.
//!
//! At the pinned revision the shared uploader's committed `cli-manifest.json`
//! is the trust anchor for the `CodeScene` CLI archive, and the action
//! *rejects* a non-empty `installer-checksum` with a hard failure rather than
//! ignoring it. A workflow that still passes the repository variable therefore
//! breaks its `CodeScene` steps the moment that variable holds anything, and
//! the variable can only ever repeat the manifest's own digest.
//!
//! Four concerns are asserted, each in its own test so a failure names the
//! defect rather than a bundle. Every one of them ranges over a collection
//! whose contents are checked first: a contract over an empty collection is
//! satisfied by deleting the thing it guards, so deleting the workflow
//! directory or the upload step fails these tests rather than passing them.
//!
//! The workflow directory is reached through a `cap_std` handle and camino
//! paths, as the repository's filesystem policy requires, so the reader states
//! the one directory it is allowed to touch rather than reaching into the
//! ambient working directory.

use anyhow::{Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};
use cap_std::{ambient_authority, fs_utf8::Dir};

/// The one approved revision of the shared uploader.
///
/// Asserted as an allowlist rather than as a floor. Ordering two commit SHAs
/// cannot be computed from a checkout, so naming the approved revision is what
/// keeps the contract hermetic; it fails closed on any other value, including
/// a tag or a branch name.
///
/// This is not the revision the estate-wide drop named, `a5765019`. That one
/// is the commit where the uploader gained its manifest trust anchor and began
/// rejecting `installer-checksum`; this repository is pinned two commits later
/// and its uploader directory is byte-identical, so the rejection and the
/// manifest are the same code. Naming `a5765019` here would mean downgrading a
/// pin that is already ahead of it.
const APPROVED_PIN: &str = "82feb2b7aac45b7efff40c9c4bb632551b14521c";

/// The uploader reference, without its revision. The `@` separator is part of
/// the marker so a differently owned action whose path merely starts with the
/// same text cannot match.
const UPLOADER_MARKER: &str = "leynos/shared-actions/.github/actions/upload-codescene-coverage@";

/// The deprecated input. It carried the SHA-256 of an installer script the
/// action no longer downloads.
const DEPRECATED_INPUT: &str = "installer-checksum";

/// The repository variable whose only consumer was that input.
const DEPRECATED_VARIABLE: &str = "CODESCENE_CLI_SHA256";

/// The `workflow_dispatch` that hashed the installer script and wrote the
/// variable back through the API, named without an extension. Other
/// repositories in the estate carry it; this contract keeps it from arriving
/// here under either GitHub extension.
const REFRESH_WORKFLOW_STEM: &str = "get-codescene-sha";

/// The path of this repository's workflow directory.
fn workflow_directory() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".github")
        .join("workflows")
}

/// A capability handle on the workflow directory, and nothing else.
fn workflow_dir() -> Result<Dir> {
    let directory = workflow_directory();
    Dir::open_ambient_dir(&directory, ambient_authority())
        .with_context(|| format!("open the workflow directory {directory}"))
}

/// Whether a directory entry's name is a GitHub workflow document.
///
/// Both extensions are accepted, and the comparison ignores case: a workflow
/// written as `.YML` is still a workflow, and a case-sensitive test would let
/// one escape every contract here without failing anything.
fn is_workflow_file(name: &str) -> bool {
    Utf8Path::new(name).extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml")
    })
}

/// Every workflow's file name and source text, in file-name order.
///
/// Fallible and assertion-free, so the repository's "no assertion in a
/// function returning `Result`" rule holds. The emptiness check that makes
/// these contracts non-vacuous lives in `read_workflow_sources` instead.
fn workflow_sources() -> Result<Vec<(String, String)>> {
    let dir = workflow_dir()?;
    let mut sources = Vec::new();
    for listed in dir.entries().context("list the workflow directory")? {
        let entry = listed.context("read a workflow directory entry")?;
        let name = entry
            .file_name()
            .context("name a workflow directory entry")?;
        if !is_workflow_file(&name) {
            continue;
        }
        let source = dir
            .read_to_string(&name)
            .with_context(|| format!("read the workflow {name}"))?;
        sources.push((name, source));
    }
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(sources)
}

/// Every workflow's file name and source text, or a failure naming the cause.
///
/// The one place a read error becomes a test failure. It also holds the
/// emptiness check: a contract that ranges over an empty collection is
/// satisfied by deleting the thing it guards, so an empty workflow directory
/// fails here rather than passing everywhere.
fn read_workflow_sources() -> Vec<(String, String)> {
    let sources = match workflow_sources() {
        Ok(sources) => sources,
        Err(error) => panic!("{error:#}"),
    };
    assert!(
        !sources.is_empty(),
        "no workflow was found under {}, so every contract in this file would pass having read \
         nothing",
        workflow_directory()
    );
    sources
}

/// The workflows whose source contains `needle`, in file-name order.
///
/// Shared by the two absence contracts below. They assert different things and
/// fail apart, but the search itself is one operation, and writing it twice
/// would leave two readers to keep in step.
fn workflows_containing(needle: &str) -> Vec<String> {
    read_workflow_sources()
        .into_iter()
        .filter(|(_, source)| source.contains(needle))
        .map(|(name, _)| name)
        .collect()
}

/// The revision of every uploader reference, paired with the workflow naming it.
///
/// Splitting on the marker rather than indexing past it keeps the reader off
/// byte offsets: a workflow is arbitrary UTF-8, and a slice taken at a
/// computed offset would panic on a multi-byte character rather than fail the
/// contract it was meant to check.
fn uploader_references() -> Vec<(String, String)> {
    read_workflow_sources()
        .into_iter()
        .flat_map(|(name, source)| {
            source
                .split(UPLOADER_MARKER)
                .skip(1)
                .map(|tail| {
                    let revision: String = tail
                        .chars()
                        .take_while(|character| !character.is_whitespace())
                        .collect();
                    (name.clone(), revision)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The uploader rejects a non-empty value, so no workflow may pass it.
///
/// This is not tidying. The action fails its own input validation on a
/// non-empty value, so both `CodeScene` steps stop working the moment the
/// variable behind the input holds anything.
#[test]
fn no_workflow_passes_the_deprecated_installer_checksum() {
    let offenders = workflows_containing(DEPRECATED_INPUT);
    assert!(
        offenders.is_empty(),
        "{DEPRECATED_INPUT} is deprecated and rejected by the uploader at {APPROVED_PIN}; remove \
         it from {offenders:?}"
    );
}

/// The variable existed only to feed the rejected input, so it must go.
///
/// Held apart from the input contract because the two regress apart: an `env`
/// line or a guard can name the variable in a workflow that passes no input at
/// all, and such a reference is what a later reader would take as evidence
/// that the variable is still wanted.
#[test]
fn no_workflow_references_the_deprecated_checksum_variable() {
    let offenders = workflows_containing(DEPRECATED_VARIABLE);
    assert!(
        offenders.is_empty(),
        "{DEPRECATED_VARIABLE} fed {DEPRECATED_INPUT} and has no remaining consumer; remove it \
         from {offenders:?}"
    );
}

/// One approved revision, so a stale pin cannot reintroduce the input.
///
/// The references are checked for content before they are checked for
/// compliance. Deleting the `CodeScene` steps would otherwise satisfy this
/// contract instead of failing it, and this repository publishes from main
/// through that action.
#[test]
fn every_uploader_reference_is_pinned_to_the_approved_revision() {
    let references = uploader_references();
    assert!(
        !references.is_empty(),
        "no upload-codescene-coverage reference was found, so the pin assertion would pass \
         vacuously; this repository is expected to call the uploader"
    );
    let wrong: Vec<&(String, String)> = references
        .iter()
        .filter(|(_, revision)| revision != APPROVED_PIN)
        .collect();
    assert!(
        wrong.is_empty(),
        "every upload-codescene-coverage reference must be pinned to {APPROVED_PIN}; found \
         {wrong:?}"
    );
}

/// Nothing reads the variable it would write, so it is dead code here.
///
/// Asserted against the directory rather than the parsed workflows: a
/// dispatch-only workflow appears in no job or step list another contract
/// reads, so its absence is the only property that can be stated.
#[test]
fn the_checksum_refresh_workflow_is_absent() {
    let dir = match workflow_dir() {
        Ok(dir) => dir,
        Err(error) => panic!("{error:#}"),
    };
    // Both extensions, aligned with the reader above. Checking only `.yml`
    // would let a `.yaml` placeholder that names no variable satisfy this
    // clause, which is precisely the shape the clause exists to catch. A real
    // refresh workflow under either extension also fails the variable clause,
    // but this one must not lean on that.
    let present: Vec<String> = ["yml", "yaml"]
        .into_iter()
        .map(|extension| format!("{REFRESH_WORKFLOW_STEM}.{extension}"))
        .filter(|name| dir.exists(name))
        .collect();
    assert!(
        present.is_empty(),
        "{present:?} maintains {DEPRECATED_VARIABLE}, which no workflow reads; delete it rather \
         than keeping a dispatch that writes an unread repository variable"
    );
}
