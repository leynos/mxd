#!/usr/bin/env bash
# Install the SynHX `hx` binary and source tree for validator runs.
#
# Usage:
#   ./scripts/install-synhx.sh
#
# Optional environment overrides:
#   HX_VERSION      Version tag to install (default: 0.1.48.1)
#   HX_WORK_ROOT    Working directory for downloads and source checkout
#                   (default: $HOME/git)
#   HX_BIN_DIR      Directory to install the `hx` binary into
#                   (default: /usr/local/bin)
#   HX_SRC_LINK     Symlink path for the extracted source tree
#                   (default: $HX_WORK_ROOT/synhx-client)

set -euo pipefail

HX_VERSION="${HX_VERSION:-0.1.48.1}"
HX_WORK_ROOT="${HX_WORK_ROOT:-$HOME/git}"
HX_BIN_DIR="${HX_BIN_DIR:-/usr/local/bin}"
HX_SRC_LINK="${HX_SRC_LINK:-$HX_WORK_ROOT/synhx-client}"

require_command() {
    local name
    name="$1"
    if ! command -v "$name" >/dev/null 2>&1; then
        echo "Error: required command not found: $name" >&2
        exit 1
    fi
}

ensure_dir() {
    local directory
    directory="$1"

    if [[ -d "$directory" ]]; then
        return
    fi

    if mkdir -p "$directory" 2>/dev/null; then
        return
    fi

    if command -v sudo >/dev/null 2>&1; then
        sudo mkdir -p "$directory"
        return
    fi

    echo "Error: cannot create directory $directory and sudo is unavailable" >&2
    exit 1
}

install_file() {
    local source destination_dir destination
    source="$1"
    destination_dir="$2"
    destination="$destination_dir/$(basename "$source")"

    if [[ -w "$destination_dir" ]]; then
        install -m 0755 "$source" "$destination"
        return
    fi

    if command -v sudo >/dev/null 2>&1; then
        sudo install -m 0755 "$source" "$destination"
        return
    fi

    echo "Error: cannot write to $destination_dir and sudo is unavailable" >&2
    echo "Set HX_BIN_DIR to a writable directory or provide sudo access." >&2
    exit 1
}

require_command tar
require_command wget
require_command install
require_command ln
require_command rm
require_command mv

ensure_dir "$HX_WORK_ROOT"
ensure_dir "$HX_BIN_DIR"
ensure_dir "$(dirname "$HX_SRC_LINK")"

binary_archive="hx-ubuntu-latest-v${HX_VERSION}.tar.gz"
binary_url="https://github.com/leynos/synhx-client/releases/download/v${HX_VERSION}/${binary_archive}"
source_archive="synhx-client-v${HX_VERSION}.tar.gz"
source_url="https://github.com/leynos/synhx-client/archive/refs/tags/v${HX_VERSION}.tar.gz"

pushd "$HX_WORK_ROOT" >/dev/null

rm -f "$binary_archive" "$source_archive" hx

wget -O "$binary_archive" "$binary_url"
tar xvf "$binary_archive"
rm -f "$binary_archive"

if [[ ! -f hx ]]; then
    echo "Error: expected extracted binary '$HX_WORK_ROOT/hx'" >&2
    exit 1
fi

install_file hx "$HX_BIN_DIR"

wget -O "$source_archive" "$source_url"
archive_listing="$(tar -tzf "$source_archive")"
source_dir="$(printf '%s\n' "$archive_listing" | sed -n '1s#/.*##p')"

if [[ -z "$source_dir" ]]; then
    echo "Error: unable to determine extracted source directory" >&2
    exit 1
fi

rm -rf "$source_dir"
tar xzf "$source_archive"
rm -f "$source_archive"

ln -sfn "$HX_WORK_ROOT/$source_dir" "$HX_SRC_LINK"

echo "hx binary is available at $HX_BIN_DIR/hx"
echo "source code is available at $(cd "$HX_SRC_LINK" && pwd)"

popd >/dev/null
