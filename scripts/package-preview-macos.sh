#!/bin/sh
# Separate, traceable preview; does not install or replace Open BIM.app.
# Usage: scripts/package-preview-macos.sh [output-dir]
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ] || [ "$(uname -m)" != arm64 ]; then
  echo "This package requires macOS ARM64." >&2
  exit 1
fi
out="${1:-dist/preview}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Commit tracked changes before packaging a preview." >&2
  exit 1
fi
revision=$(git rev-parse HEAD)
short=$(git rev-parse --short HEAD)
stage=$(mktemp -d "${TMPDIR:-/tmp}/open-bim-package.XXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM
sh scripts/package-macos.sh "$stage"
app="$stage/Open BIM Preview.app"
mv "$stage/Open BIM.app" "$app"
plist="$app/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string org.openbim.preview "$plist"
plutil -replace CFBundleName -string 'Open BIM Preview' "$plist"
plutil -replace CFBundleDisplayName -string 'Open BIM Preview' "$plist"
plutil -replace OpenBIMSourceRevision -string "$revision" "$plist"
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$plist")
cat > "$app/Contents/Resources/BUILD.txt" <<INFO
Open BIM $version — local preview for macOS ARM64
Source revision: $revision
Built UTC: $(date -u '+%Y-%m-%dT%H:%M:%SZ')
Unsigned, not notarized. Installed Open BIM.app is not replaced.
Default launch opens a disposable copy in the system temporary directory.
Copy the temporary project to a permanent folder before keeping work.
Use copies of projects when testing this preview.
INFO
cat > "$app/Contents/MacOS/Open BIM" <<'LAUNCH'
#!/bin/sh
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
if [ "$#" -eq 0 ]; then
  project_dir=$(mktemp -d "${TMPDIR:-/tmp}/open-bim-preview.XXXXXX")
  cp "$here/../Resources/examples/steel-release.obim" "$project_dir/Preview.obim"
  set -- --open "$project_dir/Preview.obim" --project-path "$project_dir/Preview.obim"
fi
exec "$here/open-bim-desktop" "$@"
LAUNCH
chmod +x "$app/Contents/MacOS/Open BIM"
plutil -lint "$plist"
archive_name="Open-BIM-$version-preview-$short-macos-arm64.zip"
archive="$out/$archive_name"
if [ -e "$archive" ]; then
  echo "Archive already exists: $archive" >&2
  exit 1
fi
ditto -c -k --keepParent "$app" "$archive"
(cd "$out" && shasum -a 256 "$archive_name" > "$archive_name.sha256")
echo "Preview: $archive"
