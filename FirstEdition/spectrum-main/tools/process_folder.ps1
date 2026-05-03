param(
    [Parameter(Mandatory=$true)]
    [string]$InputDir,

    [string]$OutputDir = "captures\processed",
    [string]$LogDir = "captures\logs",
    [string]$Config = "config\competitive_default.ini"
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$tool = Join-Path $root "build\wav_process.exe"
if (-not (Test-Path -LiteralPath $tool)) {
    throw "Missing wav_process.exe. Run build.ps1 first."
}

New-Item -ItemType Directory -Force -Path (Join-Path $root $OutputDir) | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $root $LogDir) | Out-Null

$files = Get-ChildItem -LiteralPath $InputDir -Filter *.wav -File
if ($files.Count -eq 0) {
    Write-Host "No WAV files found in $InputDir"
    exit 0
}

foreach ($file in $files) {
    $name = [System.IO.Path]::GetFileNameWithoutExtension($file.Name)
    $out = Join-Path (Join-Path $root $OutputDir) "$name.processed.wav"
    $log = Join-Path (Join-Path $root $LogDir) "$name.csv"
    Write-Host "Processing $($file.FullName)"
    & $tool $file.FullName $out (Join-Path $root $Config) $log
    if ($LASTEXITCODE -ne 0) {
        throw "Processing failed for $($file.FullName)"
    }
}

Write-Host "Processed $($files.Count) file(s)."
