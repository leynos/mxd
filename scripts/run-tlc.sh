#!/usr/bin/env bash
# Run TLC model checker via Docker
#
# Usage: ./scripts/run-tlc.sh <spec.tla> [spec.cfg]
#
# This script runs TLC in a Docker container, avoiding the need for local
# TLA+ Toolbox installation. The tlaplus/tlaplus image includes TLC, TLAPS,
# and related tools.
#
# Environment variables:
#   TLC_IMAGE - Docker image to use (default: ghcr.io/tlaplus/tlaplus:latest)
#   TLC_WORKERS - Number of worker threads (default: auto)

set -euo pipefail

TLC_IMAGE="${TLC_IMAGE:-ghcr.io/tlaplus/tlaplus:latest}"
TLC_WORKERS="${TLC_WORKERS:-auto}"

if [[ $# -lt 1 ]]; then
    echo "Usage: $0 <spec.tla> [spec.cfg]" >&2
    echo "" >&2
    echo "Run TLC model checker on a TLA+ specification via Docker." >&2
    echo "" >&2
    echo "Arguments:" >&2
    echo "  spec.tla  Path to the TLA+ specification file" >&2
    echo "  spec.cfg  Path to the TLC config file (default: spec.cfg with same basename)" >&2
    exit 1
fi

SPEC_FILE="$1"
CFG_FILE="${2:-${SPEC_FILE%.tla}.cfg}"

# Verify files exist
if [[ ! -f "$SPEC_FILE" ]]; then
    echo "Error: Specification file not found: $SPEC_FILE" >&2
    exit 1
fi

if [[ ! -f "$CFG_FILE" ]]; then
    echo "Error: Configuration file not found: $CFG_FILE" >&2
    exit 1
fi

# Run TLC in Docker
# -v mounts the current directory as /workspace
# -w sets the working directory
# --rm removes the container after exit
exec docker run --rm \
    -v "$(pwd):/workspace" \
    -w /workspace \
    "$TLC_IMAGE" \
    tlc -workers "$TLC_WORKERS" -config "$CFG_FILE" "$SPEC_FILE"
