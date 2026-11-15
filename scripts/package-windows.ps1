# Build a portable, unsigned Windows x86_64 package without overwriting a release.
# Usage: pwsh -File scripts/package-windows.ps1 [output-dir] [-SkipBuild]
# -SkipBuild packages existing binaries and records that fact in BUILD.txt.
param([string]$Out = 'dist', [switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$sourceRoot = Split-Path -Parent $PSScriptRoot
$outPath = if ([IO.Path]::IsPathRooted($Out)) {
    [IO.Path]::GetFullPath($Out)
} else {
    [IO.Path]::GetFullPath((Join-Path $sourceRoot $Out))
}
$version = (Select-String -LiteralPath (Join-Path $sourceRoot 'Cargo.toml') -Pattern '^version = "(.*)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$app = Join-Path $outPath 'Open BIM'
$archiveName = "Open-BIM-$version-windows-x86_64.zip"
$zip = Join-Path $outPath $archiveName
$checksum = "$zip.sha256"
foreach ($path in @($app, $zip, $checksum)) {
    if (Test-Path -LiteralPath $path) { throw "Output already exists; choose a new output directory: $path" }
}
Push-Location -LiteralPath $sourceRoot
try {
    if (!$SkipBuild) {
        cargo build --release --locked -p open-bim-desktop -p open-bim-reports
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed: $LASTEXITCODE" }
    }
    $files = @{
        'target/release/open-bim-desktop.exe' = 'Open BIM.exe'
        'target/release/open-bim-release.exe' = 'open-bim-release.exe'
        'target/release/open-bim-synthetic.exe' = 'open-bim-synthetic.exe'
        'tests/fixtures/steel_release/source.obim' = 'examples/steel-release.obim'
        'tests/fixtures/drawings/control.obim' = 'examples/drawing-control.obim'
        'LICENSE' = 'LICENSE'
        'README.md' = 'README.md'
        'docs/windows-interface.md' = 'Windows-interface.md'
    }
    foreach ($relative in $files.Keys) {
        if (!(Test-Path -LiteralPath (Join-Path $sourceRoot $relative) -PathType Leaf)) {
            throw "Package input is missing: $relative"
        }
    }
    $revision = 'unknown'
    $dirty = 'unknown'
    if (Get-Command git -ErrorAction SilentlyContinue) {
        $gitRoot = & git rev-parse --show-toplevel 2>$null
        if ($LASTEXITCODE -eq 0 -and [IO.Path]::GetFullPath($gitRoot) -eq [IO.Path]::GetFullPath($sourceRoot)) {
            $revision = & git rev-parse HEAD
            & git diff --quiet HEAD
            if ($LASTEXITCODE -eq 0) { $dirty = 'false' }
            elseif ($LASTEXITCODE -eq 1) { $dirty = 'true' }
            else { throw 'Unable to determine tracked source changes' }
        }
    }
    New-Item -ItemType Directory -Force -Path (Join-Path $app 'examples') | Out-Null
    foreach ($relative in $files.Keys) {
        Copy-Item -LiteralPath (Join-Path $sourceRoot $relative) -Destination (Join-Path $app $files[$relative])
    }
    @(
        "Open BIM $version - Windows x86_64"
        "Source revision: $revision"
        "Tracked source changes: $dirty"
        "Build skipped: $($SkipBuild.IsPresent.ToString().ToLowerInvariant())"
        "Release LTO override: $env:CARGO_PROFILE_RELEASE_LTO"
        "Release codegen-units override: $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS"
        "Built UTC: $([DateTime]::UtcNow.ToString('o'))"
        'Unsigned portable package.'
        'Default launch opens a user-owned project in the Windows Documents known folder.'
    ) | Set-Content -LiteralPath (Join-Path $app 'BUILD.txt') -Encoding utf8
    Compress-Archive -LiteralPath $app -DestinationPath $zip -CompressionLevel Optimal
    $hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $archiveName" | Set-Content -LiteralPath $checksum -Encoding ascii
    Write-Output "Готово: $app"
    Write-Output "Архив: $zip"
    Write-Output "SHA256: $hash"
} finally {
    Pop-Location
}
