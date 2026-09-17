#!/usr/bin/env sh
# Fetch the Pdfium shared library the `pdf` input needs, from the
# open-source pdfium-binaries project's latest release, into
# ./data/pdfium/, and print the FLASH_PDFIUM_PATH line to set.
#
#   tools/fetch-pdfium.sh                 detect the host platform
#   tools/fetch-pdfium.sh linux-musl-x64  one of: linux-x64 linux-arm64
#                                         linux-musl-x64 linux-musl-arm64
#                                         mac-arm64 mac-x64 win-x64 win-arm64
#
# Needs curl (or wget) and tar. Re-run to update.
set -eu

platform="${1:-}"
dest="${2:-./data/pdfium}"

if [ -z "$platform" ]; then
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$arch" in
        x86_64 | amd64) arch=x64 ;;
        aarch64 | arm64) arch=arm64 ;;
        *)
            echo "fetch-pdfium: no pdfium build for $os/$arch; pass a platform name" >&2
            exit 1
            ;;
    esac
    case "$os" in
        Linux)
            libc=""
            if ldd --version 2>&1 | grep -qi musl || [ -n "$(ls /lib/ld-musl-* 2>/dev/null)" ]; then
                libc="musl-"
            fi
            platform="linux-${libc}${arch}"
            ;;
        Darwin) platform="mac-$arch" ;;
        MINGW* | MSYS* | CYGWIN*) platform="win-$arch" ;;
        *)
            echo "fetch-pdfium: no pdfium build for $os; pass a platform name" >&2
            exit 1
            ;;
    esac
fi

case "$platform" in
    linux-*) member="lib/libpdfium.so" ;;
    mac-*) member="lib/libpdfium.dylib" ;;
    win-*) member="bin/pdfium.dll" ;;
    *)
        echo "fetch-pdfium: unknown platform $platform" >&2
        exit 1
        ;;
esac

url="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-${platform}.tgz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "fetch-pdfium: $url"
if command -v curl >/dev/null 2>&1; then
    curl -fsSL -o "$tmp/pdfium.tgz" "$url"
elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$tmp/pdfium.tgz" "$url"
else
    echo "fetch-pdfium: needs curl or wget" >&2
    exit 1
fi

mkdir -p "$dest"
tar -xzf "$tmp/pdfium.tgz" -C "$tmp" "$member" VERSION
cp "$tmp/$member" "$dest/"
cp "$tmp/VERSION" "$dest/VERSION"

file="$dest/$(basename "$member")"
case "$file" in
    /* | [A-Za-z]:/*) ;;
    *) file="$(pwd)/${file#./}" ;;
esac
build="$(tr -d '\r' < "$dest/VERSION" | sed -n 's/^BUILD=//p')"
echo "fetch-pdfium: chromium/${build:-?} -> $file"
echo
echo "FLASH_PDFIUM_PATH=$file"
