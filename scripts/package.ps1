param(
    [string]$Version = $env:RELEASE_VERSION
)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$pluginToml = Join-Path $root "crates/openfx-omt-plugin/Cargo.toml"
if (-not $Version) {
    $Version = (Select-String -Path $pluginToml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
}

$dist = Join-Path $root "dist"
$bundle = Join-Path $dist "OpenFXOMT.ofx.bundle"
$contents = Join-Path $bundle "Contents"
$win64 = Join-Path $contents "Win64"
$resources = Join-Path $contents "Resources"

$candidates = @(
    (Join-Path $root "target/x86_64-pc-windows-msvc/release/openfx_omt.dll"),
    (Join-Path $root "target/release/openfx_omt.dll")
)
$dll = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $dll) {
    throw "Release DLL not found. Build with: cargo build --release --locked --target x86_64-pc-windows-msvc"
}

if (Test-Path $dist) {
    Remove-Item -Recurse -Force $dist
}
New-Item -ItemType Directory -Force -Path $win64 | Out-Null
New-Item -ItemType Directory -Force -Path $resources | Out-Null

Copy-Item $dll (Join-Path $win64 "OpenFXOMT.ofx")
Copy-Item (Join-Path $root "LICENSE") (Join-Path $resources "LICENSE")
Copy-Item (Join-Path $root "THIRD_PARTY_NOTICES.md") (Join-Path $resources "THIRD_PARTY_NOTICES.md")
Copy-Item (Join-Path $root "README.md") (Join-Path $resources "README.md")

$plist = Get-Content -Raw (Join-Path $root "resources/Info.plist")
$plist = $plist -replace "<string>0\.1\.0</string>", "<string>$Version</string>"
$utf8 = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText((Join-Path $contents "Info.plist"), $plist, $utf8)

$zipName = "openfx-omt-plugin-v$Version.zip"
$zipPath = Join-Path $dist $zipName
Compress-Archive -Path $bundle -DestinationPath $zipPath
Write-Host "Wrote $zipPath"
