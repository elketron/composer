param([Parameter(Mandatory=$true)][string]$Version)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$desktop = Join-Path $root 'apps/desktop'
$cef = Join-Path $env:LOCALAPPDATA 'composer/cef/156.0.15'
$name = "composer-desktop-$Version-windows-x86_64"
$out = Join-Path $root "dist/$name"

Push-Location $desktop
try {
    if (-not (Test-Path (Join-Path $cef 'libcef.dll'))) {
        cargo install --locked export-cef-dir --version '156.3.1+156.0.15'
        if ($LASTEXITCODE -ne 0) { throw 'Installing export-cef-dir failed' }
        export-cef-dir --force $cef
        if ($LASTEXITCODE -ne 0) { throw 'Downloading CEF failed' }
    }
    $env:CEF_PATH = $cef
    $env:COMPOSER_PACKAGE = '1'
    # Rust's Windows test executables live below target/release/build, not beside CEF.
    $env:PATH = "$cef;$env:PATH"
    cargo build --release --locked --lib
    if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed' }
    cargo test --release --locked --lib
    if ($LASTEXITCODE -ne 0) { throw 'Desktop tests failed' }

    New-Item -ItemType Directory -Force $out | Out-Null
    # The sandbox bootstrap loads a DLL with the same basename as the executable.
    Copy-Item (Join-Path $cef 'bootstrap.exe') (Join-Path $out 'composer_desktop.exe')
    Copy-Item 'target/release/composer_desktop.dll' $out
    Get-ChildItem $cef -File | Where-Object {
        $_.Extension -in @('.dll', '.pak', '.bin', '.dat', '.json')
    } | Copy-Item -Destination $out
    Copy-Item (Join-Path $cef 'locales') $out -Recurse
    Copy-Item (Join-Path $cef 'CREDITS.html') (Join-Path $out 'CEF-CREDITS.html')
    Copy-Item 'assets/fonts/OFL.txt' (Join-Path $out 'FONT-LICENSE.txt')
    Copy-Item 'packaging/README-windows.txt' (Join-Path $out 'README.txt')
    Compress-Archive -Path $out -DestinationPath "$out.zip" -Force
    Write-Host "$out.zip"
} finally {
    Pop-Location
}
