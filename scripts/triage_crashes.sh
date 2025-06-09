#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
    echo "Usage: $0 <crash_dir> [harness]" >&2
    exit 1
fi

if cd "$1" 2>/dev/null; then
    CRASH_DIR="$(pwd)"
    cd - >/dev/null
else
    echo "Crash directory $1 does not exist" >&2
    exit 1
fi

HARNESS=${2:-/usr/local/bin/fuzz}

if [[ ! -x "$HARNESS" ]]; then
    echo "Harness $HARNESS is missing or not executable" >&2
    exit 1
fi

if [[ ! -d "$CRASH_DIR" ]]; then
    echo "Crash directory $CRASH_DIR does not exist" >&2
    exit 1
fi

UNIQUE_DIR="$CRASH_DIR/unique"
mkdir -p "$UNIQUE_DIR"

WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

# afl-cmin reduces crashes to those with unique coverage
# Use persistent mode harness
if ! afl-cmin -i "$CRASH_DIR" -o "$WORK_DIR" -- "$HARNESS" @@; then
    echo "afl-cmin failed" >&2
    exit 1
fi

# Minimize each crash input with afl-tmin
for crash in "$WORK_DIR"/*; do
    [ -e "$crash" ] || continue
    base=$(basename "$crash")
    if ! afl-tmin -i "$crash" -o "$UNIQUE_DIR/$base" -- "$HARNESS" @@; then
        echo "afl-tmin failed for $crash" >&2
        exit 1
    fi
done
