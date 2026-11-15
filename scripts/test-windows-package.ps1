# Exercise packaging with harmless fixture binaries, without building or running apps.
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $repoRoot ('target/windows-package-tests/' + [guid]::NewGuid().ToString('N'))
$fixture = Join-Path $testRoot 'Исходники с пробелами'
function Assert([bool]$Condition, [string]$Message) {
    if (!$Condition) { throw $Message }
}
function Assert-Refuses([scriptblock]$Action, [string]$Message) {
    $refused = $false
    try { & $Action | Out-Null } catch { $refused = $true }
    Assert $refused $Message
}
foreach ($directory in @('scripts', 'target/release', 'tests/fixtures/steel_release', 'tests/fixtures/drawings', 'docs')) {
    New-Item -ItemType Directory -Force -Path (Join-Path $fixture $directory) | Out-Null
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'package-windows.ps1') -Destination (Join-Path $fixture 'scripts/package-windows.ps1')
$packager = Join-Path $fixture 'scripts/package-windows.ps1'
'version = "0.2.2"' | Set-Content -LiteralPath (Join-Path $fixture 'Cargo.toml')
$inputs = @{
    'target/release/open-bim-desktop.exe' = 'desktop fixture'
    'target/release/open-bim-release.exe' = 'release fixture'
    'target/release/open-bim-synthetic.exe' = 'synthetic fixture'
    'tests/fixtures/steel_release/source.obim' = 'steel example'
    'tests/fixtures/drawings/control.obim' = 'drawing example'
    'LICENSE' = 'license fixture'
    'README.md' = 'readme fixture'
    'docs/windows-interface.md' = 'windows help fixture'
}
foreach ($path in $inputs.Keys) { $inputs[$path] | Set-Content -LiteralPath (Join-Path $fixture $path) -Encoding utf8 }
$output = Join-Path $testRoot 'Выпуск с пробелами'
& $packager $output -SkipBuild | Out-Null
$app = Join-Path $output 'Open BIM'
$zip = Join-Path $output 'Open-BIM-0.2.2-windows-x86_64.zip'
Assert (Test-Path -LiteralPath (Join-Path $app 'Open BIM.exe')) 'Missing application'
Assert (Test-Path -LiteralPath (Join-Path $app 'examples/steel-release.obim')) 'Missing example'
$build = Get-Content -LiteralPath (Join-Path $app 'BUILD.txt') -Raw
Assert ($build.Contains('Build skipped: true')) 'Build mode was not recorded'
Assert ($build.Contains('Source revision: unknown')) 'Fixture inherited an unrelated parent Git revision'
$hash = (Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant()
Assert ((Get-Content -LiteralPath "$zip.sha256" -Raw).StartsWith($hash + '  ')) 'Invalid archive checksum'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($zip)
try {
    $entry = $archive.Entries | Where-Object { $_.FullName -match '(^|[/\\])Open BIM\.exe$' }
    Assert (@($entry).Count -eq 1) 'Missing or duplicate executable in ZIP'
    $reader = [IO.StreamReader]::new($entry.Open())
    try { Assert ($reader.ReadToEnd().Trim() -eq 'desktop fixture') 'Wrong executable in ZIP' }
    finally { $reader.Dispose() }
} finally { $archive.Dispose() }
'keep existing app' | Set-Content -LiteralPath (Join-Path $app 'Open BIM.exe')
Assert-Refuses { & $packager $output -SkipBuild } 'Existing package was overwritten'
Assert ((Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant() -eq $hash) 'Existing ZIP changed'
Assert ((Get-Content -LiteralPath (Join-Path $app 'Open BIM.exe') -Raw).Trim() -eq 'keep existing app') 'Existing EXE changed'
$archiveOnly = Join-Path $testRoot 'Archive only'
New-Item -ItemType Directory -Path $archiveOnly | Out-Null
'keep archive' | Set-Content -LiteralPath (Join-Path $archiveOnly 'Open-BIM-0.2.2-windows-x86_64.zip')
Assert-Refuses { & $packager $archiveOnly -SkipBuild } 'Orphan archive was overwritten'
Assert (!(Test-Path -LiteralPath (Join-Path $archiveOnly 'Open BIM'))) 'Refused packaging created a partial app'
$missing = Join-Path $testRoot 'Missing inputs'
Move-Item -LiteralPath (Join-Path $fixture 'target/release/open-bim-release.exe') -Destination (Join-Path $fixture 'target/release/unavailable.exe')
Assert-Refuses { & $packager $missing -SkipBuild } 'Missing executable was accepted'
Assert (!(Test-Path -LiteralPath $missing)) 'Missing input left a partial output'
Write-Output 'Windows packaging: archive, SHA-256, metadata, overwrite protection and missing-input checks passed.'
