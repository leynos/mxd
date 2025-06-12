# CodeScene Coverage Tool

This repository uses the CodeScene CLI to report code coverage metrics in CI.
The CLI is installed from a remote script published by CodeScene. To ensure the
integrity of the download, the CI workflow verifies the script using a pinned
SHA-256 checksum.

The expected checksum is stored in the workflow as the environment variable
`CODESCENE_CLI_SHA256`. The installer is downloaded from
<https://downloads.codescene.io/enterprise/cli/install-cs-coverage-tool.sh>.
When CodeScene publishes a new installer, update `CODESCENE_CLI_SHA256` and this
document with the checksum shown in their release notes.
