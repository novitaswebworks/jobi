#!/bin/sh
# Jobi installer: downloads a prebuilt release binary, verifies its checksum, and
# installs it. Supports Linux x86_64, macOS arm64 and macOS x86_64.
#
#   curl -fsSL https://raw.githubusercontent.com/novitaswebworks/jobi/main/install.sh | sh
#
# Environment:
#   JOBI_VERSION   release tag to install, e.g. v0.2.0 (default: the latest release)
#   JOBI_BIN_DIR   where to put the binary (default: $HOME/.local/bin)
#   JOBI_BASE_URL  releases URL (default: https://github.com/novitaswebworks/jobi/releases)
set -eu

REPO="novitaswebworks/jobi"
BASE="${JOBI_BASE_URL:-https://github.com/$REPO/releases}"
BIN_DIR="${JOBI_BIN_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
die() { printf 'jobi install: %s\n' "$*" >&2; exit 1; }

fetch() { # fetch URL DEST
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$1" -O "$2"
    else
        die "need curl or wget"
    fi
}

os=$(uname -s)
arch=$(uname -m)
case "$os-$arch" in
    Linux-x86_64 | Linux-amd64) target="linux-x86_64" ;;
    Darwin-arm64 | Darwin-aarch64) target="macos-arm64" ;;
    Darwin-x86_64) target="macos-x86_64" ;;
    *)
        die "no prebuilt binary for $os $arch. Build from source instead:
  cargo install --git https://github.com/$REPO --locked"
        ;;
esac

tag="${JOBI_VERSION:-}"
if [ -z "$tag" ]; then
    command -v curl >/dev/null 2>&1 || die "set JOBI_VERSION (for example v0.2.0), or install curl"
    final=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$BASE/latest") || die "cannot reach $BASE"
    tag="${final##*/}"
    case "$tag" in v[0-9]*) ;; *) die "could not work out the latest release (got '$tag')" ;; esac
fi

archive="jobi-$tag-$target.tar.gz"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/jobi-install.XXXXXX")
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading Jobi $tag for $target..."
fetch "$BASE/download/$tag/$archive" "$tmp/$archive" || die "download failed: $BASE/download/$tag/$archive"
fetch "$BASE/download/$tag/$archive.sha256" "$tmp/$archive.sha256" || die "checksum download failed"

expected=$(cut -d ' ' -f 1 "$tmp/$archive.sha256")
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$tmp/$archive" | cut -d ' ' -f 1)
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$tmp/$archive" | cut -d ' ' -f 1)
else
    die "need sha256sum or shasum to verify the download"
fi
[ -n "$expected" ] && [ "$expected" = "$actual" ] || die "checksum mismatch; refusing to install"

tar -xzf "$tmp/$archive" -C "$tmp"
[ -f "$tmp/jobi-$tag-$target/jobi" ] || die "unexpected archive layout"
mkdir -p "$BIN_DIR"
cp "$tmp/jobi-$tag-$target/jobi" "$BIN_DIR/jobi.new"
chmod 755 "$BIN_DIR/jobi.new"
mv -f "$BIN_DIR/jobi.new" "$BIN_DIR/jobi"

say "Installed $BIN_DIR/jobi"
case ":$PATH:" in
    *":$BIN_DIR:"*) say "Run: jobi" ;;
    *) say "Add it to your PATH, e.g.:  export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac
