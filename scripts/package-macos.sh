#!/bin/sh
# Build a local, unsigned macOS application bundle «Open BIM.app».
# No code signing or notarization is performed or claimed; Gatekeeper may ask
# to confirm the first launch (right-click → Open).
#
# Usage: scripts/package-macos.sh [output-dir]   (default: dist)
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ] || [ "$(uname -m)" != arm64 ]; then
  echo "This package requires macOS ARM64." >&2
  exit 1
fi
out="${1:-dist}"
app="$out/Open BIM.app"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
archive_name="Open-BIM-$version-macos-arm64.zip"
if [ -e "$app" ] || [ -e "$out/$archive_name" ]; then
  echo "Output already exists; choose a new output directory: $out" >&2
  exit 1
fi
revision=$(git rev-parse HEAD 2>/dev/null || echo unknown)
dirty=false
if [ "$revision" != unknown ] && ! git diff --quiet HEAD; then
  dirty=true
fi
if [ -d /Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk ] && [ -z "${SDKROOT:-}" ]; then
  # Host workaround documented in README (linker rejects the newer SDK).
  export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk
fi
cargo build --release --locked -p open-bim-desktop -p open-bim-reports
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/examples"
cp target/release/open-bim-desktop "$app/Contents/MacOS/open-bim-desktop"
cp target/release/open-bim-release "$app/Contents/MacOS/open-bim-release"
cp target/release/open-bim-synthetic "$app/Contents/MacOS/open-bim-synthetic"
cp tests/fixtures/steel_release/source.obim "$app/Contents/Resources/examples/steel-release.obim"
cp tests/fixtures/drawings/control.obim "$app/Contents/Resources/examples/drawing-control.obim"
cp LICENSE README.md "$app/Contents/Resources/"
cat > "$app/Contents/MacOS/Open BIM" <<'LAUNCH'
#!/bin/sh
# Reopen the user's project, creating a separate example copy on first launch.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
if [ "$#" -eq 0 ]; then
  project="$HOME/Open BIM проект.obim"
  if [ ! -e "$project" ]; then
    temporary=$(mktemp "$HOME/.open-bim-project.XXXXXX")
    trap 'rm -f "$temporary"' EXIT
    trap 'exit 1' HUP INT TERM
    cp "$here/../Resources/examples/steel-release.obim" "$temporary"
    # A hard link publishes the complete file without overwriting a project
    # created concurrently by another launch.
    if ! /bin/link "$temporary" "$project" 2>/dev/null; then
      [ -f "$project" ] || exit 1
    fi
    rm -f "$temporary"
    trap - EXIT HUP INT TERM
  fi
  if [ ! -f "$project" ]; then
    echo "Project path is not a file: $project" >&2
    exit 1
  fi
  set -- --open "$project" --project-path "$project"
fi
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
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>OpenBIMSourceRevision</key><string>$revision</string>
  <key>OpenBIMSourceDirty</key><$dirty/>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDevelopmentRegion</key><string>ru</string>
</dict>
</plist>
PLIST
cat > "$app/Contents/Resources/BUILD.txt" <<INFO
Open BIM $version — macOS ARM64
Source revision: $revision
Tracked source changes: $dirty
Built UTC: $(date -u '+%Y-%m-%dT%H:%M:%SZ')
Unsigned, not notarized.
Default launch reopens ~/Open BIM проект.obim or creates a separate example copy.
INFO
plutil -lint "$app/Contents/Info.plist"
(cd "$out" && ditto -c -k --keepParent "Open BIM.app" "$archive_name")
(cd "$out" && shasum -a 256 "$archive_name" > "$archive_name.sha256")
echo "Готово: $app и $out/Open-BIM-$version-macos-arm64.zip (без подписи и нотаризации)"
