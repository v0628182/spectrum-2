[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$scriptRoot = Split-Path -Parent $PSCommandPath
$appRoot = Split-Path -Parent $scriptRoot
$nsisScript = Join-Path $scriptRoot 'nsis\VanySoundSetup.nsi'
$distDir = Join-Path $appRoot 'dist'
$outputExe = Join-Path $distDir 'VanySound-Setup-NSIS.exe'

$makeNsis = @(
    'C:\Program Files (x86)\NSIS\makensis.exe',
    'C:\Program Files\NSIS\makensis.exe'
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $makeNsis) {
    throw 'No se encontro makensis.exe. Instala NSIS primero.'
}

if (-not (Test-Path -LiteralPath $nsisScript)) {
    throw "No se encontro el script NSIS: $nsisScript"
}

New-Item -ItemType Directory -Path $distDir -Force | Out-Null
if (Test-Path -LiteralPath $outputExe) {
    Remove-Item -LiteralPath $outputExe -Force
}

Write-Host "Building NSIS setup with: $makeNsis"
& $makeNsis $nsisScript

if ($LASTEXITCODE -ne 0) {
    throw "makensis fallo con exit code $LASTEXITCODE"
}

if (-not (Test-Path -LiteralPath $outputExe)) {
    throw "No se genero el instalador esperado: $outputExe"
}

Write-Host "NSIS setup listo en: $outputExe"
