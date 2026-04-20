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
HX_BINARY_SHA256="${HX_BINARY_SHA256:-}"
HX_SOURCE_SHA256="${HX_SOURCE_SHA256:-}"
HX_PLATFORM_ARCHIVE=""

fail() {
    echo "Error: $*" >&2
    exit 1
}

require_supported_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    if [[ "$os" != "Linux" ]]; then
        fail "SynHX auto-install currently supports Linux only; set HX_BIN_DIR and HX_SRC_LINK manually on $os."
    fi

    case "$arch" in
        x86_64|amd64)
            HX_PLATFORM_ARCHIVE="hx-ubuntu-latest-v${HX_VERSION}.tar.gz"
            ;;
        *)
            fail "SynHX auto-install currently supports Linux x86_64 only; unsupported architecture: $arch."
            ;;
    esac
}

require_command() {
    local name
    name="$1"
    if ! command -v "$name" >/dev/null 2>&1; then
        fail "required command not found: $name"
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

    fail "cannot create directory $directory and sudo is unavailable; choose a writable path or install sudo"
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

    fail "cannot write to $destination_dir and sudo is unavailable; set HX_BIN_DIR to a writable directory or provide sudo access"
}

require_supported_platform
require_command tar
require_command wget
require_command install
require_command ln
require_command rm
require_command mv

set_expected_checksums() {
    if [[ -n "$HX_BINARY_SHA256" && -n "$HX_SOURCE_SHA256" ]]; then
        return
    fi

    case "$HX_VERSION" in
        0.1.48.1)
            : "${HX_BINARY_SHA256:=fcef7e2bff84bce4c42cab577e480a3bf788d4cca063f6ebda9df5b200c96b36}"
            : "${HX_SOURCE_SHA256:=65964cac332146214ed945f290e26bce5fcefa21ac6fea6f9edd4fdff906d965}"
            ;;
        *)
            fail "no built-in checksums for SynHX v$HX_VERSION; set HX_BINARY_SHA256 and HX_SOURCE_SHA256 explicitly"
            ;;
    esac
}

sha256_file() {
    local path checksum
    path="$1"

    if command -v sha256sum >/dev/null 2>&1; then
        read -r checksum _ < <(sha256sum "$path")
        printf '%s\n' "$checksum"
        return
    fi

    if command -v shasum >/dev/null 2>&1; then
        read -r checksum _ < <(shasum -a 256 "$path")
        printf '%s\n' "$checksum"
        return
    fi

    fail "required command not found: sha256sum or shasum"
}

verify_archive_checksum() {
    local archive expected actual
    archive="$1"
    expected="$2"

    if [[ -z "$expected" ]]; then
        fail "missing expected checksum for $archive"
    fi

    actual="$(sha256_file "$archive")"
    if [[ "$actual" == "$expected" ]]; then
        return
    fi

    rm -f "$archive"
    fail "checksum verification failed for $archive (expected $expected, got $actual)"
}

validate_source_dir() {
    local directory
    directory="$1"

    if [[ -z "$directory" ]]; then
        fail "unable to determine extracted source directory"
    fi

    if [[ "$directory" == "." || "$directory" == ".." || "$directory" == -* || "$directory" == */* ]]; then
        fail "refusing unsafe extracted source directory name: $directory"
    fi

    if [[ ! "$directory" =~ ^[A-Za-z0-9._-]+$ ]]; then
        fail "refusing extracted source directory with unexpected characters: $directory"
    fi
}

set_expected_checksums
ensure_dir "$HX_WORK_ROOT"
ensure_dir "$HX_BIN_DIR"
ensure_dir "$(dirname "$HX_SRC_LINK")"

binary_archive="$HX_PLATFORM_ARCHIVE"
binary_url="https://github.com/leynos/synhx-client/releases/download/v${HX_VERSION}/${binary_archive}"
source_archive="synhx-client-v${HX_VERSION}.tar.gz"
source_url="https://github.com/leynos/synhx-client/archive/refs/tags/v${HX_VERSION}.tar.gz"

pushd "$HX_WORK_ROOT" >/dev/null

rm -f "$binary_archive" "$source_archive" hx

wget -O "$binary_archive" "$binary_url"
verify_archive_checksum "$binary_archive" "$HX_BINARY_SHA256"
tar xvf "$binary_archive"
rm -f "$binary_archive"

if [[ ! -f hx ]]; then
    fail "expected extracted binary '$HX_WORK_ROOT/hx'"
fi

install_file hx "$HX_BIN_DIR"

wget -O "$source_archive" "$source_url"
verify_archive_checksum "$source_archive" "$HX_SOURCE_SHA256"
archive_listing="$(tar -tzf -- "$source_archive")"
mapfile -t source_dirs < <(
    printf '%s\n' "$archive_listing" |
        awk -F/ 'NF > 0 && $1 != "" { print $1 }' |
        sort -u
)

if [[ "${#source_dirs[@]}" -ne 1 ]]; then
    fail "expected exactly one top-level source directory in $source_archive"
fi

source_dir="$(basename -- "${source_dirs[0]}")"
validate_source_dir "$source_dir"

rm -rf -- "./$source_dir"
tar xzf -- "$source_archive"
rm -f "$source_archive"

if [[ ! -d "./$source_dir" ]]; then
    fail "expected extracted source directory '$HX_WORK_ROOT/$source_dir'"
fi

if [[ -e "$HX_SRC_LINK" || -L "$HX_SRC_LINK" ]]; then
    rm -rf -- "$HX_SRC_LINK"
fi

ln -sfn "$HX_WORK_ROOT/$source_dir" "$HX_SRC_LINK"

echo "hx binary is available at $HX_BIN_DIR/hx"
echo "source code is available at $(cd "$HX_SRC_LINK" && pwd)"

popd >/dev/null
