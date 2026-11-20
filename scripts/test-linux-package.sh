#!/bin/sh
# Build, unpack and check the portable and Debian packages on Linux x86_64.
set -eu
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
scripts/package-linux.sh "$tmp"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
name="Open-BIM-$version-linux-x86_64"
(
    cd "$tmp"
    sha256sum --check "$name.tar.gz.sha256" "$name.deb.sha256"
    tar -xzf "$name.tar.gz"
    dpkg-deb --extract "$name.deb" deb
    for binary in open-bim-desktop open-bim-release open-bim-synthetic; do
        test -x "$name/$binary"
        cmp "$name/$binary" "deb/opt/open-bim/$binary"
    done
    test -f "$name/examples/steel-release.obim"
    test -f "$name/examples/drawing-control.obim"
    test -f "$name/LICENSE"
    grep -F "Open BIM $version" "$name/BUILD.txt"
    grep -F 'MimeType=application/x-open-bim-project;' deb/usr/share/applications/open-bim.desktop
)
if scripts/package-linux.sh "$tmp"; then
    echo 'Существующий пакет был принят для перезаписи.' >&2
    exit 1
fi
if [ -n "${OPEN_BIM_LINUX_PACKAGE_OUTPUT:-}" ]; then
    mkdir -p "$OPEN_BIM_LINUX_PACKAGE_OUTPUT"
    cp "$tmp/$name.tar.gz" "$tmp/$name.tar.gz.sha256" "$tmp/$name.deb" "$tmp/$name.deb.sha256" "$OPEN_BIM_LINUX_PACKAGE_OUTPUT/"
fi
