#!/usr/bin/env bash
# Replay controlled inputs through the built fuzz harness and check what it
# did with each. The harness prints one outcome word per case to standard
# error when MXD_FUZZ_TRACE is set, so a harness that never hands its input
# to the parser prints nothing and fails here, even though AFL still counts
# its executions.
#
# Usage: replay_harness.sh <harness> <corpus_dir>
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "Usage: $0 <harness> <corpus_dir>" >&2
    exit 1
fi

HARNESS=$1
CORPUS=$2

if [[ ! -x "$HARNESS" ]]; then
    echo "Harness $HARNESS is missing or not executable" >&2
    exit 1
fi

# A header and a maximum-size payload: the limit the harness parses up to.
MAX_INPUT_LEN=$((20 + 1024 * 1024))

WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

# Print the harness's outcome word for the input file $1.
outcome_of() {
    MXD_FUZZ_TRACE=1 "$HARNESS" < "$1" 2>&1 >/dev/null | tail -n 1
}

# Fail unless the input file $2 gets the outcome word $1.
expect() {
    local want=$1 file=$2 got
    got=$(outcome_of "$file")
    if [[ "$got" != "$want" ]]; then
        echo "expected $want for $(basename "$file"), the harness said: ${got:-nothing}" >&2
        exit 1
    fi
}

seed=$(find "$CORPUS" -maxdepth 1 -type f -name '*.bin' | sort | head -n 1)
if [[ -z "$seed" ]]; then
    echo "No .bin seed in $CORPUS" >&2
    exit 1
fi

printf '\377\377\377' > "$WORK_DIR/malformed"
head -c $((MAX_INPUT_LEN + 1)) /dev/zero > "$WORK_DIR/oversized"

expect Accepted "$seed"
expect Rejected "$WORK_DIR/malformed"
expect Skipped "$WORK_DIR/oversized"
echo "The harness accepted a seed, rejected a malformed frame and skipped an oversized input"
