# CodeScene Coverage Tool

This repository uses the CodeScene CLI to report code coverage metrics in CI.
The shared GitHub Action
`leynos/shared-actions/.github/actions/upload-codescene-coverage` downloads,
caches, verifies and runs the CLI. `coverage-main.yml` uses it to upload the
trunk report, and no pull-request lane calls it: see "CodeScene coverage is
owned by `main`" in the developers' guide.

## Where the CLI's trust comes from

The action no longer downloads an installer script and hashes it. It resolves
the approved CLI version and its archive digest from a `cli-manifest.json`
committed alongside the action, downloads that archive, and verifies the
download against the manifest before installing or running anything. The
manifest is the trust anchor, and it moves only when the action's own pin moves.

Two consequences follow for this repository:

- There is nothing here to keep up to date when CodeScene publishes a new
  release. The approved version changes when the action's pin changes, and that
  pin is asserted by `tests/codescene_uploader_contract.rs`.
- The action's `installer-checksum` input is deprecated and a non-empty value
  is rejected with a hard failure. It is not renamed to `archive-checksum`
  here: that input can only repeat the manifest's own digest, so a repository
  variable feeding it would add nothing and would break on every manifest bump.
