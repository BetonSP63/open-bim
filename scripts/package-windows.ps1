# Build a portable, unsigned Windows x86_64 package «Open BIM» as a zip.
# No code signing is performed or claimed; SmartScreen may ask to confirm
# the first launch («Подробнее» → «Выполнить в любом случае»). An MSI
# installer is a separate step.
#
# Usage: pwsh scripts/package-windows.ps1 [output-dir]   (default: dist)
param([string]$Out = "dist")
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
cargo build --release --locked -p open-bim-desktop -p open-bim-reports
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$app = Join-Path $Out "Open BIM"
if (Test-Path $app) { Remove-Item -Recurse -Force $app }
New-Item -ItemType Directory -Force (Join-Path $app "examples") | Out-Null
# Started without arguments, the program opens examples\steel-release.obim
# and saves to «Документы\Open BIM проект.obim».
Copy-Item target/release/open-bim-desktop.exe (Join-Path $app "Open BIM.exe")
Copy-Item target/release/open-bim-release.exe $app
Copy-Item target/release/open-bim-synthetic.exe $app
Copy-Item tests/fixtures/steel_release/source.obim (Join-Path $app "examples/steel-release.obim")
Copy-Item tests/fixtures/drawings/control.obim (Join-Path $app "examples/drawing-control.obim")
Copy-Item LICENSE, README.md $app
$zip = Join-Path $Out "Open-BIM-$version-windows-x86_64.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $app -DestinationPath $zip
Write-Output "Готово: $app и $zip (без подписи)"
