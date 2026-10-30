#!/bin/sh
# Build a local, unsigned macOS application bundle «Open BIM.app».
# No code signing or notarization is performed or claimed; Gatekeeper may ask
# to confirm the first launch (right-click → Open).
#
# Usage: scripts/package-macos.sh [output-dir]   (default: dist)
set -eu
cd "$(dirname "$0")/.."
out="${1:-dist}"
app="$out/Open BIM.app"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
if [ -d /Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk ] && [ -z "${SDKROOT:-}" ]; then
  # Host workaround documented in README (linker rejects the newer SDK).
  export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk
fi
cargo build --release --locked -p open-bim-desktop -p open-bim-reports
rm -rf "${app:?}"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/examples"
cp target/release/open-bim-desktop "$app/Contents/MacOS/open-bim-desktop"
cp target/release/open-bim-release "$app/Contents/MacOS/open-bim-release"
cp target/release/open-bim-synthetic "$app/Contents/MacOS/open-bim-synthetic"
cp tests/fixtures/steel_release/source.obim "$app/Contents/Resources/examples/steel-release.obim"
cp tests/fixtures/drawings/control.obim "$app/Contents/Resources/examples/drawing-control.obim"
cp LICENSE README.md "$app/Contents/Resources/"
cp assets/icons/open-bim.icns "$app/Contents/Resources/OpenBIM.icns"
cat > "$app/Contents/MacOS/Open BIM" <<'LAUNCH'
#!/bin/sh
# Launcher: opens the bundled steel example when started without arguments.
here="$(cd "$(dirname "$0")" && pwd)"
if [ "$#" -eq 0 ]; then
  set -- --open "$here/../Resources/examples/steel-release.obim" --project-path "$HOME/Open BIM проект.obim"
fi
cd "$HOME"
exec "$here/open-bim-desktop" "$@"
LAUNCH
chmod +x "$app/Contents/MacOS/Open BIM"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>Open BIM</string>
  <key>CFBundleIdentifier</key><string>org.openbim.desktop</string>
  <key>CFBundleName</key><string>Open BIM</string>
  <key>CFBundleDisplayName</key><string>Open BIM</string>
  <key>CFBundleIconFile</key><string>OpenBIM.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDevelopmentRegion</key><string>ru</string>
</dict>
</plist>
PLIST
plutil -lint "$app/Contents/Info.plist"
(cd "$out" && ditto -c -k --keepParent "Open BIM.app" "Open-BIM-$version-macos-arm64.zip")
echo "Готово: $app и $out/Open-BIM-$version-macos-arm64.zip (без подписи и нотаризации)"
