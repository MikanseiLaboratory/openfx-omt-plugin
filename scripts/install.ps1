param(
    [string]$BundlePath,
    [string]$PluginsDir = (Join-Path ${env:ProgramFiles} "Common Files\OFX\Plugins")
)

$ErrorActionPreference = "Stop"

$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)
if (-not $isAdmin) {
    $relaunch = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $PSCommandPath
    )
    if ($PSBoundParameters.ContainsKey("BundlePath")) {
        $relaunch += @("-BundlePath", $BundlePath)
    }
    if ($PSBoundParameters.ContainsKey("PluginsDir")) {
        $relaunch += @("-PluginsDir", $PluginsDir)
    }
    $proc = Start-Process -FilePath "powershell.exe" -Verb RunAs -Wait -PassThru -ArgumentList $relaunch
    exit $proc.ExitCode
}

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
