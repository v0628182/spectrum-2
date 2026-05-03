<#
  Re-pack profiles.bin using the compiled app exe.
  Workaround: create the log directory it expects first.
#>
$ErrorActionPreference = "Stop"

$logDir = Join-Path $env:ProgramData "EchoAudio\logs"
if (-not (Test-Path $logDir)) {
    New-Item -Path $logDir -ItemType Directory -Force | Out-Null
    Write-Host "Created log dir: $logDir"
}

$exe = "C:\Users\windo\Downloads\new-rara-website\para conectar\app4\src-tauri\target\release\echoaudio-app.exe"
$profilesDir = "C:\Users\windo\Downloads\new-rara-website\para conectar\app4\equalizerAPO\Perfiles"
$outputFile = "C:\Users\windo\Downloads\new-rara-website\para conectar\app4\instalacion\profiles.bin"

Write-Host "=== Source profiles ==="
Get-ChildItem -Path $profilesDir -Recurse -File | ForEach-Object {
    Write-Host "  $($_.FullName) ($($_.Length) bytes)"
}

Write-Host "`n=== Running pack ==="
& $exe __echoaudio_native__ pack $profilesDir $outputFile

Write-Host "`n=== Output ==="
if (Test-Path $outputFile) {
    $fi = Get-Item $outputFile
    Write-Host "profiles.bin: $($fi.Length) bytes"
} else {
    Write-Host "ERROR: profiles.bin not created!"
}
