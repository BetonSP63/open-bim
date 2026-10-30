#!/bin/sh
# Run from the extracted Linux package: ./install.sh [prefix]
set -eu
package=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix="${1:-$HOME/.local}"
mkdir -p "$prefix/bin" "$prefix/share"
prefix=$(CDPATH= cd -- "$prefix" && pwd)
cp "$package/bin/open-bim-desktop" "$package/bin/open-bim-release" "$package/bin/open-bim-synthetic" "$prefix/bin/"
cp -R "$package/share/." "$prefix/share/"
# A desktop session may not include ~/.local/bin in PATH. Use the installed
# absolute path, quoted according to the Desktop Entry Exec specification.
desktop_exec=$(printf '%s' "$prefix/bin/open-bim-desktop" | sed 's/\\/\\\\\\\\/g; s/["`$]/\\\\&/g; s/%/%%/g')
while IFS= read -r line; do
  case "$line" in
    Exec=*) printf 'Exec="%s"\n' "$desktop_exec" ;;
    *) printf '%s\n' "$line" ;;
  esac
done < "$package/share/applications/org.openbim.OpenBIM.desktop" > "$prefix/share/applications/org.openbim.OpenBIM.desktop"
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$prefix/share/applications" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$prefix/share/icons/hicolor" || true
fi
echo "Installed Open BIM in $prefix (add $prefix/bin to PATH if needed)."
