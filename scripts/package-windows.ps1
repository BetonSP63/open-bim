# Build a native Windows ZIP. The EXE contains the application icon.
# Usage: pwsh -File scripts/package-windows.ps1 [-OutputDir dist]
param([string]$OutputDir = "dist")
$ErrorActionPreference = "Stop"
Push-Location (Join-Path $PSScriptRoot "..")
try {
    if (-not $IsWindows) { throw "Run this script with PowerShell 7 on Windows" }
    $manifest = Get-Content Cargo.toml -Raw
    $version = [regex]::Match($manifest, '(?m)^version = "([^"]+)"').Groups[1].Value
    $hostInfo = & rustc -vV
    if ($LASTEXITCODE -ne 0) { throw "rustc failed" }
    $targetHost = ($hostInfo | Where-Object { $_ -like "host: *" }) -replace '^host: ', ''
    $arch = ($targetHost -split '-')[0]
    & cargo build --release --locked -p open-bim-desktop -p open-bim-reports
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    $name = "Open-BIM-$version-windows-$arch"
    $package = Join-Path $OutputDir $name
    New-Item -ItemType Directory -Force -Path "$package/examples" | Out-Null
    foreach ($exe in "open-bim-desktop", "open-bim-release", "open-bim-synthetic") {
        Copy-Item "target/release/$exe.exe" $package
    }
    Copy-Item assets/icons/open-bim.ico $package
    Copy-Item tests/fixtures/steel_release/source.obim "$package/examples/steel-release.obim"
    Copy-Item tests/fixtures/drawings/control.obim "$package/examples/drawing-control.obim"
    Copy-Item LICENSE, README.md $package
    Compress-Archive -Path $package -DestinationPath "$OutputDir/$name.zip" -Force
    Write-Output "Ready: $OutputDir/$name.zip"
} finally {
    Pop-Location
}
