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
    python3 - "$name/open-bim-desktop" "$name/examples/steel-release.obim" <<'PYTEST'
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import os
import subprocess
import sys

app, example = (Path(arg).resolve() for arg in sys.argv[1:])
home = Path("Домашняя папка").resolve()
config = home / ".config"
config.mkdir(parents=True)
(config / "user-dirs.dirs").write_text('XDG_DOCUMENTS_DIR="$HOME/Мои документы"\n')
env = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(config)}

def launch(_):
    return subprocess.check_output([app, "--prepare-default-project"], env=env, text=True).strip()

with ThreadPoolExecutor(max_workers=8) as pool:
    paths = set(pool.map(launch, range(8)))
project = home / "Мои документы" / "Open BIM проект.obim"
assert paths == {str(project)}, paths
assert list(project.parent.iterdir()) == [project]
assert project.read_bytes() == example.read_bytes()
project.write_bytes(b"preserve my work")
assert launch(None) == str(project)
assert project.read_bytes() == b"preserve my work"
PYTEST
)
if scripts/package-linux.sh "$tmp"; then
    echo 'Существующий пакет был принят для перезаписи.' >&2
    exit 1
fi
if [ -n "${OPEN_BIM_LINUX_PACKAGE_OUTPUT:-}" ]; then
    mkdir -p "$OPEN_BIM_LINUX_PACKAGE_OUTPUT"
    cp "$tmp/$name.tar.gz" "$tmp/$name.tar.gz.sha256" "$tmp/$name.deb" "$tmp/$name.deb.sha256" "$OPEN_BIM_LINUX_PACKAGE_OUTPUT/"
fi
