#Requires -Version 5.1
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$issFile = Join-Path $PSScriptRoot 'bgwm.iss'
$cargoToml = Join-Path $repoRoot 'Cargo.toml'

function Get-AppVersion {
    $line = Select-String -Path $cargoToml -Pattern '^\s*version\s*=\s*"(.+)"\s*$' | Select-Object -First 1
    if (-not $line) {
        throw "Could not read package version from Cargo.toml"
    }
    return $line.Matches.Groups[1].Value
}

function Find-InnoSetupCompiler {
    $fromPath = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($fromPath) {
        return $fromPath.Source
    }

    $candidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles}\Inno Setup 6\ISCC.exe"
    )
    foreach ($path in $candidates) {
        if (Test-Path $path) {
            return $path
        }
    }
    throw @"
Inno Setup 6 not found.

Install from: https://jrsoftware.org/isdl.php
Expected ISCC.exe on PATH or in Program Files\Inno Setup 6\
"@
}

$version = Get-AppVersion
$iscc = Find-InnoSetupCompiler

Write-Host "[BGWM] Building release binary..."
Push-Location $repoRoot
try {
    & cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

Write-Host "[BGWM] Compiling installer (v$version)..."
& $iscc "/DMyAppVersion=$version" $issFile
if ($LASTEXITCODE -ne 0) {
    throw "ISCC failed with exit code $LASTEXITCODE"
}

$installer = Join-Path $repoRoot "dist\bgwm-setup-$version.exe"
Write-Host "[BGWM] Installer ready: $installer"
