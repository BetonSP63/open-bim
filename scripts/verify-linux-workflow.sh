#!/bin/sh
# Run the packaged app under Xvfb and verify the saved outputs.
set -eu
if [ "$#" -ne 2 ]; then
    echo 'Использование: scripts/verify-linux-workflow.sh /путь/к/open-bim-desktop /путь/к/отчёту' >&2
    exit 1
fi
repo=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
app=$(realpath "$1")
test -x "$app"
mkdir -p "$2"
results=$(realpath "$2")
if [ -e "$results/workflow.obim" ] || [ -e "$results/journal.json" ]; then
    echo "Выберите пустую папку результатов: $results" >&2
    exit 1
fi
mkdir -p "$results/screenshots" "$results/home/runtime"
chmod 700 "$results/home/runtime"
python3 - "$repo/tests/ui/modeling-workflow/script.json" "$results" <<'PY'
import json
from pathlib import Path
import sys

source, output = map(Path, sys.argv[1:])
steps = json.loads(source.read_text())
for step in steps:
    if isinstance(step, dict) and "capture" in step:
        step["capture"] = str(output / "screenshots" / Path(step["capture"]).name)
(output / "script.json").write_text(json.dumps(steps, ensure_ascii=False))
PY
cd "$results"
export XDG_RUNTIME_DIR="$results/home/runtime"
export LIBGL_ALWAYS_SOFTWARE=1
{
    uname -a
    if command -v vulkaninfo >/dev/null; then vulkaninfo --summary; fi
} > platform.log
timeout 180s env HOME="$results/home" xvfb-run -a --server-args='-screen 0 1600x1000x24' \
    "$app" --size 1280x800 --project-path "$results/workflow.obim" \
    --record "$results/journal.json" --script "$results/script.json" > app.log 2>&1
python3 "$repo/tests/ui/modeling-workflow/check_outputs.py" "$results" > check.txt
cat check.txt
