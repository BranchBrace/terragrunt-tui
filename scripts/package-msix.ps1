<#
.SYNOPSIS
  Construit le paquet MSIX de « ttui for Terragrunt » à soumettre au Microsoft Store.

.DESCRIPTION
  À lancer sur un poste Windows (toolchain Rust MSVC et Windows SDK installés), depuis le
  tag de la release stable à publier. Produit dist\ttui-for-terragrunt-X.Y.Z.0-x64.msix,
  non signé : le Store signe le paquet après certification. Le paquet se téléverse ensuite
  dans Partner Center (Soumission > Packages).

.PARAMETER Version
  Version stable X.Y.Z ; par défaut celle de Cargo.toml. Le Store impose une version à
  4 parties qui augmente à chaque soumission : le paquet porte X.Y.Z.0.
#>
param([string]$Version)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

if (-not $Version) {
  $match = Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.+)"$' | Select-Object -First 1
  $Version = $match.Matches[0].Groups[1].Value
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
  throw "Le Store ne publie que des versions stables X.Y.Z (reçu : $Version). Passer -Version."
}
$packageVersion = "$Version.0"

# Outils du Windows SDK (le plus récent installé), ou dossier imposé par WINDOWS_KITS_PATH.
$kit = $env:WINDOWS_KITS_PATH
if (-not $kit) {
  $kit = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64\makeappx.exe" -ErrorAction SilentlyContinue |
    Sort-Object { [version]$_.Directory.Parent.Name } | Select-Object -Last 1 | ForEach-Object { $_.DirectoryName }
}
if (-not $kit -or -not (Test-Path (Join-Path $kit 'makeappx.exe'))) {
  throw 'makeappx.exe introuvable : installer le Windows SDK ou définir WINDOWS_KITS_PATH.'
}
$makeappx = Join-Path $kit 'makeappx.exe'
$makepri = Join-Path $kit 'makepri.exe'

Push-Location $root
try {
  cargo build --release --locked --bin terragrunt-tui
  if ($LASTEXITCODE) { throw "cargo build a échoué ($LASTEXITCODE)." }

  $stage = Join-Path $root 'target\msix\stage'
  if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
  New-Item -ItemType Directory -Force $stage | Out-Null
  Copy-Item 'target\release\terragrunt-tui.exe', 'LICENSE' $stage
  Copy-Item -Recurse 'packaging\msix\Assets' $stage
  (Get-Content -Raw 'packaging\msix\AppxManifest.xml').Replace('@VERSION@', $packageVersion) |
    Set-Content -NoNewline -Encoding utf8 (Join-Path $stage 'AppxManifest.xml')

  # Index des ressources : sans lui, Windows ne trouve pas les logos qualifiés (scale-200, targetsize-16…).
  $config = Join-Path $root 'target\msix\priconfig.xml'
  & $makepri createconfig /cf $config /dq fr-FR /pv 10.0.0 /o | Out-Null
  if ($LASTEXITCODE) { throw "makepri createconfig a échoué ($LASTEXITCODE)." }
  & $makepri new /pr $stage /cf $config /mn (Join-Path $stage 'AppxManifest.xml') /of (Join-Path $stage 'resources.pri') /o | Out-Null
  if ($LASTEXITCODE) { throw "makepri new a échoué ($LASTEXITCODE)." }

  New-Item -ItemType Directory -Force 'dist' | Out-Null
  $msix = Join-Path $root "dist\ttui-for-terragrunt-$packageVersion-x64.msix"
  & $makeappx pack /d $stage /p $msix /o
  if ($LASTEXITCODE) { throw "makeappx pack a échoué ($LASTEXITCODE)." }
  Write-Host "Paquet prêt : $msix"
} finally {
  Pop-Location
}
