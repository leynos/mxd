#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
    echo "Usage: $0 <crash_dir> [harness]" >&2
    exit 1
fi

CRASH_DIR=$(realpath "$1")
HARNESS=${2:-/usr/local/bin/fuzz}

if [[ ! -d "$CRASH_DIR" ]]; then
    echo "Crash directory $CRASH_DIR does not exist" >&2
    exit 0
fi

UNIQUE_DIR="$CRASH_DIR/unique"
mkdir -p "$UNIQUE_DIR"

WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

# afl-cmin reduces crashes to those with unique coverage
# Use persistent mode harness
afl-cmin -i "$CRASH_DIR" -o "$WORK_DIR" -- "$HARNESS" @@ || true

# Minimize each crash input with afl-tmin
for crash in "$WORK_DIR"/*; do
    [ -e "$crash" ] || continue
    base=$(basename "$crash")
    afl-tmin -i "$crash" -o "$UNIQUE_DIR/$base" -- "$HARNESS" @@ || true
done
