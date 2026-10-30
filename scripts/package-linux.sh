#!/bin/sh
# Build a native Linux tarball. Run install.sh inside it to add the app menu icon.
# Usage: scripts/package-linux.sh [output-dir] (default: dist)
set -eu
cd "$(dirname "$0")/.."
[ "$(uname -s)" = Linux ] || { echo "Run this script on Linux" >&2; exit 1; }
out="${1:-dist}"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
name="Open-BIM-$version-linux-$(uname -m)"
package="$out/$name"
cargo build --release --locked -p open-bim-desktop -p open-bim-reports
mkdir -p "$package/bin" "$package/share/applications" "$package/share/open-bim/examples"
cp target/release/open-bim-desktop target/release/open-bim-release target/release/open-bim-synthetic "$package/bin/"
cp assets/linux/org.openbim.OpenBIM.desktop "$package/share/applications/"
for size in 16 24 32 48 64 128 256 512 1024; do
  icons="$package/share/icons/hicolor/${size}x${size}/apps"
  mkdir -p "$icons"
  cp "assets/icons/${size}x${size}.png" "$icons/org.openbim.OpenBIM.png"
done
cp tests/fixtures/steel_release/source.obim "$package/share/open-bim/examples/steel-release.obim"
cp tests/fixtures/drawings/control.obim "$package/share/open-bim/examples/drawing-control.obim"
cp LICENSE README.md "$package/share/open-bim/"
cp scripts/install-linux.sh "$package/install.sh"
chmod +x "$package/install.sh"
tar -czf "$out/$name.tar.gz" -C "$out" "$name"
echo "Ready: $out/$name.tar.gz"
