# CodeScene Coverage Tool

This repository uses the CodeScene CLI to report code coverage metrics in CI.
A shared GitHub Action (`leynos/shared-actions/upload-codescene-coverage@v1.0.3`)
handles downloading and caching the CLI before uploading coverage results. The
action verifies the installer using a pinned SHA-256 checksum.

The expected checksum is stored in the workflow as the environment variable
`CODESCENE_CLI_SHA256`. The installer is downloaded from
<https://downloads.codescene.io/enterprise/cli/install-cs-coverage-tool.sh>.
When CodeScene publishes a new installer, update `CODESCENE_CLI_SHA256` and this
document with the checksum shown in their release notes.
