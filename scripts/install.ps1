param(
    [string]$BundlePath,
    [string]$PluginsDir = (Join-Path ${env:ProgramFiles} "Common Files\OFX\Plugins")
)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$bundleName = "OpenFXOMT.ofx.bundle"
if (-not $BundlePath) {
    $BundlePath = Join-Path $root "dist\$bundleName"
}

if (-not (Test-Path $BundlePath)) {
    throw "Bundle not found at '$BundlePath'. Build first: cargo build --release --locked --target x86_64-pc-windows-msvc; ./scripts/package.ps1"
}

$dest = Join-Path $PluginsDir $bundleName
New-Item -ItemType Directory -Force -Path $PluginsDir | Out-Null
if (Test-Path $dest) {
    Remove-Item -Recurse -Force $dest
}
Copy-Item -Recurse $BundlePath $dest
Write-Host "Installed $bundleName to $dest"
Write-Host "Restart DaVinci Resolve to load the plugin."
