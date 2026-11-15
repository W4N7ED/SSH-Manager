# ─────────────────────────────────────────────────────────────────────────────
# Produit les livrables Windows dans dist\ :
#   SSH-Manager-<version>-Portable.exe   version portable (données à côté)
#   SSH-Manager-Setup-<version>.exe      installeur NSIS
#
# Usage : powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1
#
# makensis est cherché dans le PATH, puis dans l'installation NSIS standard,
# puis dans le cache d'electron-builder.
# ─────────────────────────────────────────────────────────────────────────────

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Get-WorkspaceVersion {
    $manifest = Get-Content (Join-Path $root 'Cargo.toml') -Raw
    $section = [regex]::Match($manifest, '(?ms)^\[workspace\.package\](.*?)^\[')
    $version = [regex]::Match($section.Groups[1].Value, 'version\s*=\s*"([^"]+)"')
    if (-not $version.Success) { throw "Version introuvable dans [workspace.package] de Cargo.toml" }
    return $version.Groups[1].Value
}

function Find-Makensis {
    $candidates = @(
        (Get-Command makensis -ErrorAction SilentlyContinue).Source,
        "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
        "$env:ProgramFiles\NSIS\makensis.exe"
    )
    $cache = Join-Path $env:LOCALAPPDATA 'electron-builder\Cache\nsis'
    if (Test-Path $cache) {
        $candidates += Get-ChildItem $cache -Recurse -Filter makensis.exe |
            Where-Object { $_.DirectoryName -notmatch '\\(linux|mac)\\' } |
            Select-Object -ExpandProperty FullName
    }
    $found = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $found) { throw "makensis introuvable : installez NSIS (https://nsis.sourceforge.io)" }
    return $found
}

$version = Get-WorkspaceVersion
$dist = Join-Path $root 'dist'
$staging = Join-Path $dist 'staging'
Write-Host "Version $version"

Write-Host 'Compilation (release)…'
cargo build --release -p ssh-manager
if ($LASTEXITCODE -ne 0) { throw "La compilation a échoué (code $LASTEXITCODE)" }
$executable = Join-Path $root 'target\release\ssh-manager.exe'

if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
New-Item -ItemType Directory -Force (Join-Path $staging 'licences') | Out-Null
Copy-Item $executable $staging
Copy-Item (Join-Path $root 'LICENSE') $staging
Copy-Item (Join-Path $root 'assets\icons\icon.ico') $staging
Copy-Item (Join-Path $root 'assets\fonts\*-OFL.txt') (Join-Path $staging 'licences')

# Le mot « Portable » dans le nom suffit à activer le mode portable.
$portable = Join-Path $dist "SSH-Manager-$version-Portable.exe"
Copy-Item $executable $portable -Force
Write-Host "Portable  : $portable"

$makensis = Find-Makensis
$setup = Join-Path $dist "SSH-Manager-Setup-$version.exe"
& $makensis /V2 /INPUTCHARSET UTF8 "/DVERSION=$version" "/DSOURCE_DIR=$staging" "/DOUTPUT_FILE=$setup" (Join-Path $root 'installer\ssh-manager.nsi')
if ($LASTEXITCODE -ne 0) { throw "makensis a échoué (code $LASTEXITCODE)" }
Write-Host "Installeur : $setup"

Remove-Item $staging -Recurse -Force
