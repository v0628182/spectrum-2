#Requires -Version 5.1

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$localHelper = Join-Path $scriptDir "VanySoundControl.exe"
$installedHelper = "C:\Program Files\VanySoundEngine\VanySoundControl.exe"
$legacyLocal = Join-Path $scriptDir "EchoAudioControl.exe"
$legacyInstalled = "C:\Program Files\EchoAudioEngine\EchoAudioControl.exe"

function Resolve-HelperPath {
    foreach ($candidate in @($localHelper, $installedHelper, $legacyLocal, $legacyInstalled)) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    throw "No se encontro VanySoundControl.exe para reparar el Device Selector."
}

function Ensure-Administrator {
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        return
    }

    Start-Process powershell.exe `
        -ArgumentList "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$($MyInvocation.MyCommand.Definition)`"" `
        -Verb RunAs `
        -Wait `
        -WindowStyle Hidden
    exit
}

Ensure-Administrator
$helper = Resolve-HelperPath
& $helper repair-device-selector
exit $LASTEXITCODE
