#!/bin/sh
# Build portable and Debian packages on Linux x86_64.
set -eu
cd "$(dirname "$0")/.."
case "$(uname -s)/$(uname -m)" in
    Linux/x86_64) ;;
    *) echo "Сборка пакета требует Linux x86_64." >&2; exit 1 ;;
esac
out=${1:-dist}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
test -n "$version"
name="Open-BIM-$version-linux-x86_64"
for file in "$name.tar.gz" "$name.tar.gz.sha256" "$name.deb" "$name.deb.sha256"; do
    if [ -e "$out/$file" ] || [ -L "$out/$file" ]; then
        echo "Файл $out/$file уже существует. Выберите другую папку." >&2
        exit 1
    fi
done
cargo build --release --locked -p open-bim-desktop -p open-bim-reports
target=${CARGO_TARGET_DIR:-target}/release
revision=$(git rev-parse HEAD)
dirty=false
if [ -n "$(git status --porcelain)" ]; then dirty=true; fi
mkdir -p "$out"
stage=$(mktemp -d "$out/.open-bim-linux.XXXXXX")
trap 'rm -rf "$stage"' EXIT
trap 'exit 1' HUP INT TERM
app="$stage/$name"
mkdir -p "$app/examples"
cp "$target/open-bim-desktop" "$target/open-bim-release" "$target/open-bim-synthetic" "$app/"
cp tests/fixtures/steel_release/source.obim "$app/examples/steel-release.obim"
cp tests/fixtures/drawings/control.obim "$app/examples/drawing-control.obim"
cp LICENSE README.md docs/dependencies.md "$app/"
cat > "$app/BUILD.txt" <<INFO
Open BIM $version — Linux x86_64
Source revision: $revision
Source changes: $dirty
Built UTC: $(date -u '+%Y-%m-%dT%H:%M:%SZ')
INFO
tar -czf "$stage/$name.tar.gz" -C "$stage" "$name"

deb="$stage/deb"
install -d "$deb/DEBIAN" "$deb/opt" "$deb/usr/bin" "$deb/usr/share/applications" \
    "$deb/usr/share/mime/packages" "$deb/usr/share/icons/hicolor/scalable/apps"
mv "$app" "$deb/opt/open-bim"
ln -s /opt/open-bim/open-bim-desktop "$deb/usr/bin/open-bim-desktop"
install -m 644 scripts/linux/open-bim.desktop "$deb/usr/share/applications/"
install -m 644 scripts/linux/open-bim.xml "$deb/usr/share/mime/packages/"
install -m 644 scripts/linux/open-bim.svg "$deb/usr/share/icons/hicolor/scalable/apps/"
cat > "$deb/DEBIAN/control" <<CONTROL
Package: open-bim
Version: $version
Architecture: amd64
Maintainer: Open BIM <open-bim@localhost>
Depends: libc6, libgcc-s1, libstdc++6, libxkbcommon0
Recommends: libvulkan1, mesa-vulkan-drivers, xdg-desktop-portal
Description: Open BIM structural modeling and documentation
CONTROL
dpkg-deb --root-owner-group --build "$deb" "$stage/$name.deb"
(
    cd "$stage"
    for package in "$name.tar.gz" "$name.deb"; do
        sha256sum "$package" > "$package.sha256"
    done
)
for file in "$name.tar.gz" "$name.tar.gz.sha256" "$name.deb" "$name.deb.sha256"; do
    ln "$stage/$file" "$out/$file"
done
