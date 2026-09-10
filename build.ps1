$ErrorActionPreference = 'Stop'
# Local build script for QingRead: frontend + Rust release binary + NSIS installer.
#
# Toolchain resolution order:
#   1. .tools\cargo-home next to this script (a vendored toolchain, if present)
#   2. whatever cargo/rustup is already on PATH
# Step 1 exists because on some Windows setups MinGW's ld cannot read library
# paths that contain non-ASCII characters. A vendored toolchain under an ASCII
# path avoids that; see the README for details.
#
# NOTE: keep this file ASCII-only. Windows PowerShell 5.1 reads .ps1 files without
# a BOM using the system ANSI code page, and mis-decoded comment bytes can silently
# break the statements that follow them.

$repoRoot = $PSScriptRoot
if (-not $repoRoot) { $repoRoot = (Get-Location).Path }

$tools = Join-Path $repoRoot '.tools'
$vendoredCargoHome = Join-Path $tools 'cargo-home'
$vendoredCargo = Join-Path $vendoredCargoHome 'bin\cargo.exe'

if (Test-Path -LiteralPath $vendoredCargo) {
    $cargoHome = $vendoredCargoHome
    $rustupHome = Join-Path $tools 'rustup-home'
    [System.Environment]::SetEnvironmentVariable('CARGO_HOME', $cargoHome)
    [System.Environment]::SetEnvironmentVariable('RUSTUP_HOME', $rustupHome)
    [System.Environment]::SetEnvironmentVariable('PATH', ((Join-Path $cargoHome 'bin') + ';' + $env:PATH))
    Write-Host 'Using vendored toolchain under .tools'
} else {
    $systemCargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $systemCargo) {
        Write-Error ('No Rust toolchain found. Install one from https://rustup.rs, or vendor it under ' + $vendoredCargoHome)
    }
    Write-Host ('Using system toolchain: ' + $systemCargo.Source)
}

# Read the version the bundle will be named after, so the report matches reality.
$confPath = Join-Path $repoRoot 'src-tauri\tauri.conf.json'
$version = $null
if (Test-Path -LiteralPath $confPath) {
    try { $version = (Get-Content -Raw -LiteralPath $confPath | ConvertFrom-Json).version } catch { $version = $null }
}

Write-Host 'QingRead local build'
Write-Host ('  repo        : ' + $repoRoot)
Write-Host ('  version     : ' + $version)
Write-Host ('  CARGO_HOME  : ' + $env:CARGO_HOME)
Write-Host ('  RUSTUP_HOME : ' + $env:RUSTUP_HOME)
Write-Host ('  cargo       : ' + (Get-Command cargo -ErrorAction SilentlyContinue).Source)

if ($env:QINGREAD_BUILD_PROBE -eq '1') { exit 0 }

Push-Location $repoRoot
try {
    if (-not (Test-Path -LiteralPath (Join-Path $repoRoot 'node_modules'))) {
        Write-Host 'Installing frontend dependencies...'
        npm.cmd ci
        if ($LASTEXITCODE -ne 0) { throw ('npm ci failed with exit code ' + $LASTEXITCODE) }
    }
    Write-Host 'Building frontend, Rust release binary and NSIS installer...'
    npm.cmd run tauri build
    if ($LASTEXITCODE -ne 0) { throw ('tauri build failed with exit code ' + $LASTEXITCODE) }
} finally {
    Pop-Location
}

$exe = Join-Path $repoRoot 'src-tauri\target\release\qingread.exe'
$nsisDir = Join-Path $repoRoot 'src-tauri\target\release\bundle\nsis'

# Prefer the installer named after the current version; older ones may still sit in
# the directory from previous builds.
$setup = $null
if ($version) {
    $expected = Join-Path $nsisDir ('QingRead_' + $version + '_x64-setup.exe')
    if (Test-Path -LiteralPath $expected) { $setup = Get-Item -LiteralPath $expected }
}
if (-not $setup) {
    $setup = Get-ChildItem -LiteralPath $nsisDir -Filter '*setup.exe' -ErrorAction SilentlyContinue |
        Sort-Object -Property LastWriteTime -Descending | Select-Object -First 1
}
$stale = @()
if ($version) {
    $stale = Get-ChildItem -LiteralPath $nsisDir -Filter '*setup.exe' -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -ne ('QingRead_' + $version + '_x64-setup.exe') }
}

Write-Host ''
Write-Host 'Build finished:'
if (Test-Path -LiteralPath $exe) {
    $exeItem = Get-Item -LiteralPath $exe
    Write-Host ('  EXE      : ' + $exeItem.FullName + '  (' + [math]::Round($exeItem.Length / 1MB, 2) + ' MB)')
}
if ($setup) {
    Write-Host ('  Installer: ' + $setup.FullName + '  (' + [math]::Round($setup.Length / 1MB, 2) + ' MB)')
}
if ($stale.Count -gt 0) {
    Write-Host ''
    Write-Host 'Stale installers from earlier versions (safe to delete):'
    foreach ($s in $stale) { Write-Host ('  ' + $s.Name) }
}
