#Requires -Version 5.1
<#
.SYNOPSIS
    Instalacion silenciosa de Equalizer APO 1.4.2 + control unificado VanySound.

.DESCRIPTION
    1. Instala Equalizer APO via NSIS /S sin UI interactiva.
    2. Detecta el endpoint Hi-Fi Cable y registra el APO wrapper en el dispositivo.
    3. Copia MJUCjr.dll al stack del APO.
    4. Compila/copía VanySoundControl.exe.
    5. Genera/despliega profiles.bin y materializa solo el perfil activo.
    6. Limpia la distribucion stock (Editor.exe, Qt, shortcuts, etc.).
    7. Registra un daemon privilegiado para atender switch/status/verify.
#>

param(
    [switch]$ConsoleLog,
    [switch]$SkipSelfElevation,
    [string]$DesktopInstallDir = "C:\Program Files\VanySound",
    [string]$DesktopExeName = "VanySound.exe"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "SilentlyContinue"

$LOG_FILE  = Join-Path $env:TEMP "vanysound_apo.log"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$nativeSentinel = "__vanysound_native__"
$desktopExeCandidates = @($DesktopExeName, "vanysound-app.exe", "VanySound.exe", "app3.exe") |
    Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
    Select-Object -Unique

$apoExe        = Join-Path $scriptDir "equalizerapo\EqualizerAPO-x64-1.4.2.exe"
if (-not (Test-Path $apoExe)) {
    $apoExe = Join-Path $scriptDir "EqualizerAPO-x64-1.4.2.exe"
}
$mjucSrc       = Join-Path $scriptDir "equalizerapo\MJUCjr.dll"
if (-not (Test-Path $mjucSrc)) {
    $mjucSrc = Join-Path $scriptDir "MJUCjr.dll"
}
$apoInstDir    = "C:\Program Files\EqualizerAPO"
$apoConfig     = Join-Path $apoInstDir "config"
$apoVst        = Join-Path $apoInstDir "VSTPlugins"
$engineRoot    = "C:\Program Files\VanySoundEngine"
$controlSrc    = Join-Path $scriptDir "VanySoundControl.cs"
$controlExe    = Join-Path $scriptDir "VanySoundControl.exe"
$profilesRoot  = Join-Path $scriptDir "..\equalizerAPO\Perfiles"
$profilesBundle = Join-Path $scriptDir "profiles.bin"
$embeddedGenerator = Join-Path $scriptDir "generate_embedded_profiles.ps1"
$embeddedBuildScript = Join-Path $scriptDir "build_embedded_engine.ps1"
$embeddedArtifactsRoot = Join-Path $scriptDir "..\vendor\EqualizerAPO\artifacts\EmbeddedEngine"
$embeddedEngineVersion = "embedded-engine-v1"
$installedControlExe = Join-Path $engineRoot "VanySoundControl.exe"

$CLSID_FX_PreMix   = "{EACD2258-FCAC-4FF4-B36D-419E924A6D79}"
$CLSID_FX_PreProc  = "{EC1CC9CE-FAED-4822-828A-82A81A6F018F}"
$CLSID_FX_PostMix  = "{5860E1C5-F95C-4a7a-8EC8-8AEF24F379A1}"
$CLSID_APO_LFX_APO  = "{62dc1a93-ae24-464c-a43e-452f824c4250}"
$CLSID_APO_LFX_PROC = "{637c490d-eee3-4c0a-973f-371958802da2}"
$apoGuidValueNames = @(
    "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1",
    "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},2",
    "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5",
    "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6",
    "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7"
)
$fxPostMixValueName = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},3"
$fxInstallBlob1ValueName = "{fc52a749-4be9-4510-896e-966ba6525980},3"
$fxInstallBlob2ValueName = "{9c00eeed-edce-4cd8-ae08-cb05e8ef57a0},3"
$fxInstallDwordValueName = "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5"
$installBlob1Bytes = [byte[]](0x0b, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00)
$installBlob2Bytes = [byte[]](0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00)
$disableAutoAdjustValueName = "DisableAutomaticAdjustment"
$allowSilentBufferValueName = "AllowSilentBufferModification"
$preMixChildValueName = "PreMixChild"
$postMixChildValueName = "PostMixChild"
$versionValueName = "Version"
$preferredHiFiNeedles = @(
    "echo plus hi-fi",
    "vb-audio hi-fi cable",
    "hi-fi cable",
    "hifi cable",
    "echo plus",
    "hi-fi"
)
$secondaryHiFiNeedles = @(
    "vanysound.com",
    "vanysound",
    "echoaudio.com",
    "echoaudio"
)
$excludedHiFiNeedles = @(
    "voicemeeter",
    "vaio",
    "microphone",
    "mic",
    "stream",
    "chat",
    "aux"
)

function Write-Log {
    param([string]$Msg, [string]$Level = "INFO")
    $ts = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $line = "[$ts][$Level] $Msg"
    Add-Content -Path $LOG_FILE -Value $line -Encoding UTF8
    if ($ConsoleLog) {
        try { [Console]::Out.WriteLine($line) } catch {}
        try { [Console]::Out.Flush() } catch {}
    }
}

function Resolve-CscPath {
    $candidates = @(
        "C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe",
        "C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe"
    )

    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    $cmd = Get-Command csc.exe -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }

    Write-Log "CRITICAL: csc.exe no encontrado en rutas conocidas." "ERROR"
    exit 1
}

function Invoke-PowerShellFile {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$LogPrefix = "PS"
    )

    $psArgs = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $FilePath
    ) + $ArgumentList

    $output = & powershell.exe @psArgs 2>&1
    $exitCode = $LASTEXITCODE
    foreach ($line in @($output)) {
        if (-not [string]::IsNullOrWhiteSpace($line)) {
            Write-Log "$LogPrefix :: $line"
        }
    }

    return [pscustomobject]@{
        ExitCode = $exitCode
        Output   = @($output)
    }
}

function Resolve-NativeControlAppPath {
    $parentRoot = [System.IO.Path]::GetFullPath((Join-Path $scriptDir ".."))
    $candidateDirs = @(
        $DesktopInstallDir,
        $parentRoot,
        (Join-Path $parentRoot "VanySound"),
        (Join-Path $scriptDir "VanySound"),
        (Join-Path $parentRoot "EchoAudio"),
        (Join-Path $scriptDir "EchoAudio")
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique

    foreach ($dir in $candidateDirs) {
        foreach ($exeName in $desktopExeCandidates) {
            $candidate = Join-Path $dir $exeName
            if (Test-Path $candidate) {
                return $candidate
            }
        }
    }

    return $null
}

function Decode-PropVariant {
    param([object]$raw)
    if ($null -eq $raw) { return $null }
    if ($raw -is [string]) { return $raw }
    if ($raw -is [byte[]] -and $raw.Length -ge 10) {
        try {
            $vt = [BitConverter]::ToUInt16($raw, 0)
            if ($vt -eq 31) {
                return ([System.Text.Encoding]::Unicode.GetString($raw, 8, $raw.Length - 8)).TrimEnd([char]0).Trim()
            }
        } catch {}
        try {
            $s = [System.Text.Encoding]::Unicode.GetString($raw)
            if ($s -match "(?i)hi.?fi|vb-audio.+cable|echo.?plus|echoaudio|vanysound") {
                return $s.Trim([char]0).Trim()
            }
        } catch {}
    }
    return $null
}

function Get-HiFiEndpointMatchScore {
    param([string]$Text)

    if ([string]::IsNullOrWhiteSpace($Text)) {
        return 0
    }

    $normalized = $Text.Trim().ToLowerInvariant()
    foreach ($needle in $excludedHiFiNeedles) {
        if ($normalized.Contains($needle)) {
            return 0
        }
    }

    for ($i = 0; $i -lt $preferredHiFiNeedles.Count; $i++) {
        if ($normalized.Contains($preferredHiFiNeedles[$i])) {
            return 100 - $i
        }
    }

    for ($i = 0; $i -lt $secondaryHiFiNeedles.Count; $i++) {
        if ($normalized.Contains($secondaryHiFiNeedles[$i])) {
            return 20 - $i
        }
    }

    return 0
}

function Resolve-HiFiRenderEndpoint {
    $renderBase = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render"
    $best = $null

    foreach ($dev in (Get-ChildItem $renderBase -ErrorAction SilentlyContinue)) {
        $guid = $dev.PSChildName
        $propsPath = Join-Path $dev.PSPath "Properties"
        $props = Get-ItemProperty $propsPath -ErrorAction SilentlyContinue
        if (-not $props) {
            continue
        }

        $bestScore = 0
        $bestName = $null
        foreach ($prop in $props.PSObject.Properties) {
            if ($prop.Name -like "PS*") {
                continue
            }

            $decoded = Decode-PropVariant $prop.Value
            $score = Get-HiFiEndpointMatchScore $decoded
            if ($score -gt $bestScore) {
                $bestScore = $score
                $bestName = $decoded
            }
        }

        if ($bestScore -le 0) {
            continue
        }

        if (-not $best -or $bestScore -gt $best.Score) {
            $best = [pscustomobject]@{
                Guid = $guid
                Name = if ($bestName) { $bestName } else { $guid }
                Score = $bestScore
            }
        }
    }

    return $best
}

function Get-HiFiGuidsPnP {
    param([string]$DeviceType)
    $result = @()
    $typeTag = if ($DeviceType -eq "Render") { '0\.0\.0\.' } else { '0\.0\.1\.' }
    try {
        $eps = Get-PnpDevice -Class AudioEndpoint -ErrorAction SilentlyContinue |
            Where-Object {
                $_.InstanceId -match "MMDEVAPI" -and
                $_.InstanceId -match $typeTag -and
                ($_.FriendlyName -match "(?i)hi.?fi|vb-audio.+cable|echo.?plus|echoaudio|vanysound")
            }
        foreach ($ep in $eps) {
            $m = [Regex]::Match($ep.InstanceId, '\{([0-9a-fA-F-]{36})\}$')
            if ($m.Success) {
                $guid = "{0}" -f ("{" + $m.Groups[1].Value + "}")
                if ($result -notcontains $guid) { $result += $guid }
                Write-Log "  [PnP] $DeviceType '$($ep.FriendlyName)' =] $guid"
            }
        }
    } catch {
        Write-Log "  [PnP] Error: $_" "WARN"
    }
    return $result
}

function Get-HiFiGuidsReg {
    param([string]$DeviceType)
    $result = @()
    $base = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$DeviceType"
    foreach ($dev in (Get-ChildItem $base -ErrorAction SilentlyContinue)) {
        $guid = $dev.PSChildName
        $props = Get-ItemProperty "$($dev.PSPath)\Properties" -ErrorAction SilentlyContinue
        if (-not $props) { continue }
        foreach ($pn in $props.PSObject.Properties.Name) {
            if ($pn -like "PS*") { continue }
            $str = Decode-PropVariant ($props.$pn)
            if ($str -and $str -match "(?i)hi.?fi|vb-audio.+cable|echo.?plus|echoaudio|vanysound") {
                if ($result -notcontains $guid) { $result += $guid }
                Write-Log "  [Reg] $DeviceType GUID=$guid via '$pn'='$str'"
                break
            }
        }
    }
    return $result
}

function Get-RegistryPropertyValue {
    param([string]$Path, [string]$Name)

    try {
        $item = Get-ItemProperty -Path $Path -ErrorAction SilentlyContinue
        if (-not $item) { return $null }
        $prop = $item.PSObject.Properties[$Name]
        if (-not $prop) { return $null }
        return [string]$prop.Value
    } catch {
        return $null
    }
}

function Get-RegistryRawPropertyValue {
    param([string]$Path, [string]$Name)

    try {
        $item = Get-ItemProperty -Path $Path -ErrorAction SilentlyContinue
        if (-not $item) { return $null }
        $prop = $item.PSObject.Properties[$Name]
        if (-not $prop) { return $null }
        return $prop.Value
    } catch {
        return $null
    }
}

function Test-ByteArrayEqual {
    param(
        [byte[]]$Left,
        [byte[]]$Right
    )

    if ($null -eq $Left -or $null -eq $Right) {
        return $false
    }
    if ($Left.Length -ne $Right.Length) {
        return $false
    }

    for ($i = 0; $i -lt $Left.Length; $i++) {
        if ($Left[$i] -ne $Right[$i]) {
            return $false
        }
    }

    return $true
}

function Get-ApoBackupValue {
    param([string]$FxPath, [string]$ValueName)

    if (-not (Test-Path $FxPath)) {
        return "!KEY"
    }

    $currentValue = Get-RegistryPropertyValue -Path $FxPath -Name $ValueName
    if ([string]::IsNullOrEmpty($currentValue)) {
        return "!VALUE"
    }

    if ($currentValue -ieq $CLSID_FX_PreMix -or $currentValue -ieq $CLSID_FX_PreProc) {
        return "!VALUE"
    }

    return $currentValue
}

function Set-DeviceSelectorInstallState {
    param(
        [string]$DeviceType,
        [string]$Guid,
        [bool]$InstallPostMix = $true
    )

    $deviceBase = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$DeviceType\$Guid"
    $fxPath = "$deviceBase\FxProperties"
    $childRoot = "HKLM:\SOFTWARE\EqualizerAPO\Child APOs"
    $childPath = "$childRoot\$Guid"

    if (-not (Test-Path $fxPath)) {
        New-Item -Path $fxPath -Force | Out-Null
    }
    if (-not (Test-Path $childRoot)) {
        New-Item -Path $childRoot -Force | Out-Null
    }
    if (-not (Test-Path $childPath)) {
        New-Item -Path $childPath -Force | Out-Null
    }

    foreach ($valueName in $apoGuidValueNames) {
        $backupValue = Get-ApoBackupValue -FxPath $fxPath -ValueName $valueName
        New-ItemProperty -Path $childPath -Name $valueName -Value $backupValue -PropertyType String -Force | Out-Null
    }

    New-ItemProperty -Path $childPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1" -Value $CLSID_APO_LFX_APO -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $childPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},2" -Value $CLSID_APO_LFX_PROC -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $childPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5" -Value $CLSID_APO_LFX_APO -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $childPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6" -Value $CLSID_APO_LFX_PROC -PropertyType String -Force | Out-Null
    Remove-ItemProperty -Path $childPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7" -ErrorAction SilentlyContinue
    New-ItemProperty -Path $childPath -Name $preMixChildValueName -Value $CLSID_APO_LFX_APO -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $childPath -Name $postMixChildValueName -Value $CLSID_APO_LFX_PROC -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $childPath -Name $allowSilentBufferValueName -Value "false" -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $childPath -Name $versionValueName -Value "2" -PropertyType String -Force | Out-Null

    if ((Get-ItemProperty -Path $childPath -Name $disableAutoAdjustValueName -ErrorAction SilentlyContinue)) {
        Remove-ItemProperty -Path $childPath -Name $disableAutoAdjustValueName -ErrorAction SilentlyContinue
    }

    New-ItemProperty -Path $fxPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1" -Value $CLSID_FX_PreMix -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $fxPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},2" -Value $CLSID_FX_PreProc -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $fxPath -Name $fxPostMixValueName -Value $CLSID_FX_PostMix -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $fxPath -Name $fxInstallBlob1ValueName -Value $installBlob1Bytes -PropertyType Binary -Force | Out-Null
    New-ItemProperty -Path $fxPath -Name $fxInstallBlob2ValueName -Value $installBlob2Bytes -PropertyType Binary -Force | Out-Null
    New-ItemProperty -Path $fxPath -Name $fxInstallDwordValueName -Value 0 -PropertyType DWord -Force | Out-Null

    foreach ($unusedValueName in @(
        "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5",
        "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6",
        "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7"
    )) {
        Remove-ItemProperty -Path $fxPath -Name $unusedValueName -ErrorAction SilentlyContinue
    }

    Write-Log "DeviceSelector state aplicado para $DeviceType $Guid -> PreMix=ON PostMix=$InstallPostMix OriginalAPO=OFF Auto=ON LFX/GFX SilentBuffer=OFF"
}

function Test-DeviceSelectorInstallState {
    param(
        [string]$DeviceType,
        [string]$Guid
    )

    $deviceBase = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$DeviceType\$Guid"
    $fxPath = "$deviceBase\FxProperties"
    $childPath = "HKLM:\SOFTWARE\EqualizerAPO\Child APOs\$Guid"

    if (-not (Test-Path $fxPath) -or -not (Test-Path $childPath)) {
        return $false
    }

    if ((Get-RegistryPropertyValue -Path $fxPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1") -ine $CLSID_FX_PreMix) {
        return $false
    }
    if ((Get-RegistryPropertyValue -Path $fxPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},2") -ine $CLSID_FX_PreProc) {
        return $false
    }
    if ((Get-RegistryPropertyValue -Path $fxPath -Name $fxPostMixValueName) -ine $CLSID_FX_PostMix) {
        return $false
    }

    foreach ($unusedValueName in @(
        "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5",
        "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6",
        "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7"
    )) {
        if ($null -ne (Get-RegistryRawPropertyValue -Path $fxPath -Name $unusedValueName)) {
            return $false
        }
    }

    $blob1 = Get-RegistryRawPropertyValue -Path $fxPath -Name $fxInstallBlob1ValueName
    $blob2 = Get-RegistryRawPropertyValue -Path $fxPath -Name $fxInstallBlob2ValueName
    $installDword = Get-RegistryRawPropertyValue -Path $fxPath -Name $fxInstallDwordValueName
    if (-not (Test-ByteArrayEqual -Left ([byte[]]$blob1) -Right $installBlob1Bytes)) {
        return $false
    }
    if (-not (Test-ByteArrayEqual -Left ([byte[]]$blob2) -Right $installBlob2Bytes)) {
        return $false
    }
    if ([int]$installDword -ne 0) {
        return $false
    }

    if ((Get-RegistryPropertyValue -Path $childPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1") -ine $CLSID_APO_LFX_APO) {
        return $false
    }
    if ((Get-RegistryPropertyValue -Path $childPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},2") -ine $CLSID_APO_LFX_PROC) {
        return $false
    }
    if ((Get-RegistryPropertyValue -Path $childPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5") -ine $CLSID_APO_LFX_APO) {
        return $false
    }
    if ((Get-RegistryPropertyValue -Path $childPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6") -ine $CLSID_APO_LFX_PROC) {
        return $false
    }
    if ($null -ne (Get-RegistryRawPropertyValue -Path $childPath -Name "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7")) {
        return $false
    }
    if ((Get-RegistryPropertyValue -Path $childPath -Name $preMixChildValueName) -ine $CLSID_APO_LFX_APO) {
        return $false
    }
    if ((Get-RegistryPropertyValue -Path $childPath -Name $postMixChildValueName) -ine $CLSID_APO_LFX_PROC) {
        return $false
    }
    if ((Get-RegistryPropertyValue -Path $childPath -Name $allowSilentBufferValueName) -ine "false") {
        return $false
    }
    if ([string](Get-RegistryPropertyValue -Path $childPath -Name $versionValueName) -ne "2") {
        return $false
    }
    if ($null -ne (Get-RegistryRawPropertyValue -Path $childPath -Name $disableAutoAdjustValueName)) {
        return $false
    }

    return $true
}

function Clear-DeviceSelectorInstallState {
    param(
        [string]$DeviceType,
        [string]$Guid
    )

    $deviceBase = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$DeviceType\$Guid"
    $fxPath = "$deviceBase\FxProperties"
    $childPath = "HKLM:\SOFTWARE\EqualizerAPO\Child APOs\$Guid"
    $uiPath = "HKCU:\SOFTWARE\EqualizerAPO\Configuration Editor"

    foreach ($valueName in @(
        "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5",
        "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6",
        "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7"
    )) {
        Remove-ItemProperty -Path $fxPath -Name $valueName -ErrorAction SilentlyContinue
    }

    if (Test-Path $childPath) {
        Remove-Item -Path $childPath -Recurse -Force -ErrorAction SilentlyContinue
    }

    $selectedDevice = Get-RegistryPropertyValue -Path $uiPath -Name "selectedDevice"
    if ($selectedDevice -and $selectedDevice -match [Regex]::Escape($Guid.Trim('{}'))) {
        Remove-ItemProperty -Path $uiPath -Name "selectedDevice" -ErrorAction SilentlyContinue
        Remove-ItemProperty -Path $uiPath -Name "selectedChannelMask" -ErrorAction SilentlyContinue
    }

    Write-Log "Device Selector omitido/limpiado para $DeviceType $Guid."
}

function Install-ControlPlane {
    if (-not (Test-Path $engineRoot)) {
        New-Item $engineRoot -ItemType Directory -Force | Out-Null
    }

    try {
        & schtasks.exe /End /TN "VanySoundControl_Daemon" 2>$null | Out-Null
        & schtasks.exe /End /TN "EchoAudioControl_Daemon" 2>$null | Out-Null
    } catch {}
    Get-Process -Name "VanySoundControl","EchoAudioControl" -ErrorAction SilentlyContinue | ForEach-Object {
        try {
            Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
            Write-Log "VanySoundControl detenido para actualizar binario. pid=$($_.Id)"
        } catch {}
    }

    if (Test-Path $controlExe) {
        Copy-Item $controlExe $installedControlExe -Force
        Write-Log "VanySoundControl.exe copiado desde artefacto precompilado: $installedControlExe"
        return
    }

    # Fallback: use the native app (echoaudio-app.exe) which has control functionality built-in
    $nativeApp = Resolve-NativeControlAppPath
    if ($nativeApp) {
        Copy-Item $nativeApp $installedControlExe -Force
        Write-Log "Control plane: using native app as control helper: $nativeApp =] $installedControlExe"
        return
    }

    Write-Log "WARN: VanySoundControl.exe not found and no native app available. Control plane commands will use native app at runtime." "WARN"
}

function Sync-EmbeddedProfilesSource {
    if (-not (Test-Path $embeddedGenerator)) {
        Write-Log "WARN: generador de perfiles embebidos no encontrado: $embeddedGenerator" "WARN"
        return
    }

    $result = Invoke-PowerShellFile -FilePath $embeddedGenerator -LogPrefix "EMBEDDED PROFILES"
    if ($result.ExitCode -ne 0) {
        Write-Log "WARN: No se pudieron regenerar los perfiles embebidos (exit=$($result.ExitCode))." "WARN"
        return
    }

    Write-Log "Perfiles embebidos regenerados en vendor\\EqualizerAPO."
}

function Ensure-ProfilesBundle {
    $sourceTreeAvailable = Test-Path $profilesRoot
    $needsBuild = -not (Test-Path $profilesBundle)
    if (-not $needsBuild) {
        $bundleTime = (Get-Item $profilesBundle).LastWriteTimeUtc
        $newerSource = @()

        if ($sourceTreeAvailable) {
            $newerSource += Get-ChildItem $profilesRoot -Recurse -File -ErrorAction SilentlyContinue |
                Where-Object { $_.LastWriteTimeUtc -gt $bundleTime } |
                Select-Object -First 1
        }

        if ($sourceTreeAvailable -and (-not $newerSource) -and (Test-Path $installedControlExe)) {
            $newerSource = Get-Item $installedControlExe | Where-Object { $_.LastWriteTimeUtc -gt $bundleTime }
        }

        $needsBuild = [bool]$newerSource
    }

    if (-not $needsBuild) {
        Write-Log "profiles.bin encontrado: $profilesBundle"
        return
    }

    if (-not $sourceTreeAvailable) {
        if (Test-Path $profilesBundle) {
            Write-Log "profiles.bin empaquetado encontrado; se omite regeneracion desde perfiles fuente."
            return
        }
        Write-Log "CRITICAL: Perfiles fuente no encontrados en $profilesRoot" "ERROR"
        exit 1
    }

    $proc = Start-Process $installedControlExe `
        -ArgumentList "pack `"$profilesRoot`" `"$profilesBundle`"" `
        -Wait -PassThru -WindowStyle Hidden
    if ($proc.ExitCode -ne 0) {
        Write-Log "CRITICAL: No se pudo generar profiles.bin (exit=$($proc.ExitCode))" "ERROR"
        exit 1
    }
    Write-Log "profiles.bin generado desde $profilesRoot"
}

function Install-EmbeddedEngineIfAvailable {
    $artifactDir = Join-Path $embeddedArtifactsRoot "x64\Release"
    $embeddedDll = Join-Path $artifactDir "EqualizerAPO.dll"
    if (-not (Test-Path $embeddedDll)) {
        if (Test-Path $embeddedBuildScript) {
            Write-Log "Intentando compilar fork embebido de Equalizer APO..."
            $result = Invoke-PowerShellFile -FilePath $embeddedBuildScript -ArgumentList @("-Configuration", "Release", "-Platform", "x64") -LogPrefix "EMBEDDED BUILD"
            if ($result.ExitCode -ne 0) {
                Write-Log "WARN: Compilacion del engine embebido fallo (exit=$($result.ExitCode)); se usara fallback stock." "WARN"
                return
            }
        }
    }

    if (-not (Test-Path $embeddedDll)) {
        Write-Log "No hay build del engine embebido disponible; se mantiene APO stock." "WARN"
        return
    }

    if (-not (Test-Path $engineRoot)) {
        New-Item -ItemType Directory -Path $engineRoot -Force | Out-Null
    }

    Get-ChildItem -Path $artifactDir -Filter "*.dll" | ForEach-Object {
        Copy-Item $_.FullName (Join-Path $apoInstDir $_.Name) -Force
        Copy-Item $_.FullName (Join-Path $engineRoot $_.Name) -Force
    }
    if (Test-Path (Join-Path $artifactDir "EqualizerAPO.pdb")) {
        Copy-Item (Join-Path $artifactDir "EqualizerAPO.pdb") (Join-Path $engineRoot "EqualizerAPO.pdb") -Force
    }

    if (-not (Test-Path "HKLM:\SOFTWARE\VanySound")) {
        New-Item "HKLM:\SOFTWARE\VanySound" -Force | Out-Null
    }
    New-ItemProperty -Path "HKLM:\SOFTWARE\VanySound" -Name "EngineVersion" -Value $embeddedEngineVersion -PropertyType String -Force | Out-Null
    Write-Log "Fork embebido instalado en $apoInstDir con runtimes auxiliares."
}

function Invoke-ControlCommand {
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [string]$Prefix = "CONTROL"
    )

    $nativeApp = Resolve-NativeControlAppPath
    $helperLabel = $installedControlExe
    if ($nativeApp) {
        $output = & $nativeApp $nativeSentinel @Arguments 2>&1
        $helperLabel = "$nativeApp $nativeSentinel"
    } else {
        $output = & $installedControlExe @Arguments 2>&1
    }
    $exitCode = $LASTEXITCODE
    foreach ($line in @($output)) {
        if (-not [string]::IsNullOrWhiteSpace($line)) {
            Write-Log "$Prefix :: $line"
        }
    }

    return [pscustomobject]@{
        ExitCode = $exitCode
        Helper   = $helperLabel
        Output   = @($output)
    }
}

function Parse-ControlOutputMap {
    param([string[]]$Lines)

    $map = @{}
    foreach ($line in @($Lines)) {
        if ($line -match '^([A-Z0-9_]+)=(.*)$') {
            $map[$matches[1]] = $matches[2]
        }
    }
    return $map
}

function Get-ControlSelectorHealth {
    param([hashtable]$StatusMap)

    $targetEndpointGuid = [string]$StatusMap["TARGET_ENDPOINT_GUID"]
    $helperVersion = [string]$StatusMap["HELPER_VERSION"]
    $helperSelectorActive = ([string]$StatusMap["DEVICE_SELECTOR_ACTIVE"]).Trim().ToLowerInvariant() -eq "true"
    $helperSelectorDetail = ([string]$StatusMap["DEVICE_SELECTOR_DETAIL"]).Trim()
    $registrySelectorActive = $false

    if ($targetEndpointGuid -match '^\{[0-9A-Fa-f-]+\}$') {
        $registrySelectorActive = Test-DeviceSelectorInstallState -DeviceType "Render" -Guid $targetEndpointGuid
    }

    return [pscustomobject]@{
        TargetEndpointGuid = $targetEndpointGuid
        HelperVersion = $helperVersion
        HelperSelectorActive = $helperSelectorActive
        HelperSelectorDetail = $helperSelectorDetail
        RegistrySelectorActive = $registrySelectorActive
    }
}

function Ensure-ControlSelectorActive {
    param(
        [hashtable]$StatusMap,
        [string]$Reason = "post-install"
    )

    $selectorHealth = Get-ControlSelectorHealth -StatusMap $StatusMap
    if ($selectorHealth.TargetEndpointGuid -notmatch '^\{[0-9A-Fa-f-]+\}$') {
        return $StatusMap
    }

    if ($selectorHealth.HelperSelectorActive -and $selectorHealth.RegistrySelectorActive) {
        return $StatusMap
    }

    if ($selectorHealth.RegistrySelectorActive `
        -and $selectorHealth.HelperVersion -eq "control-plane-v2" `
        -and $selectorHealth.HelperSelectorDetail -eq "not-managed") {
        Write-Log "Compatibilidad v2: Device Selector activo en registro aunque el helper reporte not-managed durante $Reason. Se continuara con advertencia." "WARN"
        return $StatusMap
    }

    Write-Log "WARN: Device Selector inactivo durante $Reason. helper=$($selectorHealth.HelperSelectorActive) registry=$($selectorHealth.RegistrySelectorActive) endpoint=$($selectorHealth.TargetEndpointGuid). Se reaplicara el endpoint." "WARN"
    Set-DeviceSelectorInstallState -DeviceType "Render" -Guid $selectorHealth.TargetEndpointGuid -InstallPostMix $true
    Start-Sleep -Milliseconds 800

    $statusRetry = Invoke-ControlCommand -Arguments @("status") -Prefix "CONTROL STATUS RETRY"
    Write-Log "CONTROL STATUS RETRY exit=$($statusRetry.ExitCode)"
    if ($statusRetry.ExitCode -ne 0) {
        Write-Log "CRITICAL: VanySoundControl status fallo tras reparar Device Selector durante $Reason." "ERROR"
        exit 1
    }

    $statusRetryMap = Parse-ControlOutputMap -Lines $statusRetry.Output
    $retryHealth = Get-ControlSelectorHealth -StatusMap $statusRetryMap
    if ($retryHealth.RegistrySelectorActive `
        -and $retryHealth.HelperVersion -eq "control-plane-v2" `
        -and $retryHealth.HelperSelectorDetail -eq "not-managed") {
        Write-Log "Compatibilidad v2: Device Selector verificado por registro despues de reparar, aunque el helper siga reportando not-managed." "WARN"
        return $statusRetryMap
    }

    if (-not ($retryHealth.HelperSelectorActive -and $retryHealth.RegistrySelectorActive)) {
        Write-Log "CRITICAL: Device Selector sigue inactivo despues de reparar. helper=$($retryHealth.HelperSelectorActive) registry=$($retryHealth.RegistrySelectorActive) endpoint=$($retryHealth.TargetEndpointGuid)" "ERROR"
        exit 1
    }

    Write-Log "Device Selector activo despues de reparar durante $Reason."
    return $statusRetryMap
}

function Invoke-VerifyOrRepairControlPlane {
    param([string]$Reason = "post-install")

    $statusResult = Invoke-ControlCommand -Arguments @("status") -Prefix "CONTROL STATUS"
    Write-Log "CONTROL STATUS exit=$($statusResult.ExitCode)"
    if ($statusResult.ExitCode -ne 0) {
        Write-Log "CRITICAL: VanySoundControl status fallo durante $Reason (exit=$($statusResult.ExitCode))." "ERROR"
        exit 1
    }

    $statusMap = Parse-ControlOutputMap -Lines $statusResult.Output
    $profilesToTry = New-Object System.Collections.Generic.List[string]
    [void]$profilesToTry.Add("1")

    $activeProfile = [string]$statusMap["ACTIVE_PROFILE"]
    if ($activeProfile -match '^[1-4]$' -and $activeProfile -ne "1") {
        [void]$profilesToTry.Add($activeProfile)
    }

    $statusMap = Ensure-ControlSelectorActive -StatusMap $statusMap -Reason $Reason

    $verifyResult = Invoke-ControlCommand -Arguments @("verify") -Prefix "CONTROL VERIFY"
    if ($verifyResult.ExitCode -eq 0) {
        $verifyMap = Parse-ControlOutputMap -Lines $verifyResult.Output
        [void](Ensure-ControlSelectorActive -StatusMap $verifyMap -Reason "$Reason verify")
        Write-Log "VanySoundControl verify OK durante $Reason."
        return
    }

    Write-Log "WARN: VanySoundControl verify fallo durante $Reason (exit=$($verifyResult.ExitCode)); se intentara rematerializar el perfil." "WARN"

    foreach ($profileId in ($profilesToTry | Select-Object -Unique)) {
        Write-Log "Intentando reparar materializacion con profile=$profileId..."
        $switchResult = Invoke-ControlCommand -Arguments @("switch", $profileId) -Prefix "CONTROL REPAIR SWITCH"
        Write-Log "CONTROL REPAIR SWITCH exit=$($switchResult.ExitCode) profile=$profileId"
        if ($switchResult.ExitCode -ne 0) {
            continue
        }

        Start-Sleep -Milliseconds 600
        $verifyAfterRepair = Invoke-ControlCommand -Arguments @("verify") -Prefix "CONTROL VERIFY RETRY"
        if ($verifyAfterRepair.ExitCode -eq 0) {
            $verifyAfterRepairMap = Parse-ControlOutputMap -Lines $verifyAfterRepair.Output
            [void](Ensure-ControlSelectorActive -StatusMap $verifyAfterRepairMap -Reason "$Reason verify-retry")
            Write-Log "Reparacion OK: verify paso despues de switch $profileId."
            return
        }
    }

    Write-Log "CRITICAL: VanySoundControl verify sigue fallando despues de rematerializar el perfil." "ERROR"
    exit 1
}

function Set-StoredHiFiEndpoint {
    param(
        [Parameter(Mandatory = $true)][string]$Guid,
        [string]$Name = "VanySound"
    )

    if (-not (Test-Path "HKLM:\SOFTWARE\VanySound")) {
        New-Item "HKLM:\SOFTWARE\VanySound" -Force | Out-Null
    }

    New-ItemProperty -Path "HKLM:\SOFTWARE\VanySound" -Name "HiFiEndpointGuid" -Value $Guid -PropertyType String -Force | Out-Null
    New-ItemProperty -Path "HKLM:\SOFTWARE\VanySound" -Name "HiFiEndpointName" -Value $Name -PropertyType String -Force | Out-Null
    Write-Log "Endpoint objetivo guardado en HKLM:\\SOFTWARE\\VanySound =] $Guid ($Name)"
}

function Resolve-StoredHiFiEndpoint {
    try {
        $echoState = Get-ItemProperty "HKLM:\SOFTWARE\VanySound" -ErrorAction Stop
        $guid = [string]$echoState.HiFiEndpointGuid
        if ([string]::IsNullOrWhiteSpace($guid)) {
            return $null
        }

        $renderPath = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$guid"
        if (-not (Test-Path $renderPath)) {
            Write-Log "WARN: HiFiEndpointGuid almacenado pero sin MMDevices render: $guid" "WARN"
            return $null
        }

        return [pscustomobject]@{
            Guid = $guid
            Name = if ([string]::IsNullOrWhiteSpace([string]$echoState.HiFiEndpointName)) { $guid } else { [string]$echoState.HiFiEndpointName }
            Score = 999
        }
    } catch {
        return $null
    }
}

function Resolve-HiFiRenderEndpointRobust {
    $stored = Resolve-StoredHiFiEndpoint
    if ($stored) {
        Write-Log "Usando endpoint almacenado: $($stored.Name) =] $($stored.Guid)"
        return $stored
    }

    $scored = Resolve-HiFiRenderEndpoint
    if ($scored) {
        Write-Log "Endpoint encontrado por scoring directo: $($scored.Name) =] $($scored.Guid)"
        Set-StoredHiFiEndpoint -Guid $scored.Guid -Name $scored.Name
        return $scored
    }

    $fallbackGuids = @()
    $fallbackGuids += Get-HiFiGuidsPnP -DeviceType "Render"
    $fallbackGuids += Get-HiFiGuidsReg -DeviceType "Render"
    $fallbackGuids = @($fallbackGuids | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)
    if ($fallbackGuids.Count -gt 0) {
        $guid = [string]$fallbackGuids[0]
        Write-Log "Endpoint encontrado por fallback PnP/registro: $guid"
        Set-StoredHiFiEndpoint -Guid $guid -Name "VanySound"
        return [pscustomobject]@{
            Guid = $guid
            Name = "VanySound"
            Score = 50
        }
    }

    return $null
}

function Ensure-TargetEndpointReady {
    Write-Log "Resolviendo endpoint objetivo con PnP/registro..."

    $candidate = Resolve-HiFiRenderEndpointRobust
    if ($candidate) {
        Set-StoredHiFiEndpoint -Guid $candidate.Guid -Name $candidate.Name
        return $candidate
    }

    return $null
}

function Deploy-ProfilesBundle {
    $deployedBundlePath = "C:\ProgramData\VanySound\profiles.bin"
    $attempts = @(
        @{ Name = "initial-deploy" },
        @{ Name = "retry-without-device-selector" }
    )

    foreach ($attempt in $attempts) {
        Write-Log "Intentando deploy de profiles.bin ($($attempt.Name))..."
        $deployResult = Invoke-ControlCommand -Arguments @("deploy", $profilesBundle) -Prefix "CONTROL DEPLOY"
        $deployExit = $deployResult.ExitCode

        if ($deployExit -eq 0) {
            Write-Log "profiles.bin desplegado via VanySoundControl."
            return
        }

        Write-Log "WARN: Deploy de profiles.bin fallo en '$($attempt.Name)' (exit=$deployExit)." "WARN"
    }

    if (Test-Path $deployedBundlePath) {
        Write-Log "WARN: profiles.bin quedo copiado en $deployedBundlePath aunque el deploy no pudo materializar el perfil inicial." "WARN"
        Write-Log "Intentando dejar APO en estado 'cleared' para completar la instalacion..."
        $clearResult = Invoke-ControlCommand -Arguments @("clear") -Prefix "CONTROL CLEAR"
        Write-Log "CONTROL CLEAR exit=$($clearResult.ExitCode)"
        if ($clearResult.ExitCode -eq 0) {
            Write-Log "Fallback OK: bundle desplegado y config inicial limpiada; la app podra reaplicar o reparar despues." "WARN"
            return
        }

        Write-Log "WARN: El fallback 'clear' tambien fallo, pero el bundle ya quedo instalado para recuperacion posterior." "WARN"
        return
    }

    Write-Log "CRITICAL: Deploy de profiles.bin fallo despues de retry y no se detecto bundle instalado." "ERROR"
    exit 1
}

function Register-ControlDaemon {
    try {
        & schtasks /Delete /TN "VanySound_ConfigGuard" /F 2>$null
        & schtasks /Delete /TN "VanySoundControl_Daemon" /F 2>$null
        & schtasks /Delete /TN "EchoAudio_ConfigGuard" /F 2>$null
        & schtasks /Delete /TN "EchoAudioControl_Daemon" /F 2>$null
    } catch {}

    try {
        & schtasks /Create `
            /TN "VanySoundControl_Daemon" `
            /TR "`"$installedControlExe`" serve" `
            /SC ONSTART `
            /RU "SYSTEM" `
            /RL HIGHEST `
            /F 2>$null
        Write-Log "Tarea VanySoundControl_Daemon registrada."
    } catch {
        Write-Log "WARN: No se pudo registrar VanySoundControl_Daemon: $_" "WARN"
    }

    try {
        Start-Process $installedControlExe -ArgumentList "serve" -WindowStyle Hidden
        Write-Log "VanySoundControl daemon iniciado."
    } catch {
        Write-Log "WARN: No se pudo iniciar VanySoundControl daemon: $_" "WARN"
    }
}

function Apply-DistributionCleanup {
    Write-Log "Se conserva la huella estandar de Equalizer APO para instalacion y desinstalacion limpias."
    Get-ChildItem $apoInstDir -Filter "*.url" -ErrorAction SilentlyContinue | ForEach-Object {
        Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue
        Write-Log "  URL promocional eliminada: $($_.Name)"
    }
}

$esAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $esAdmin -and $SkipSelfElevation) {
    Write-Log "ERROR: SkipSelfElevation fue solicitado pero el proceso no esta elevado." "ERROR"
    exit 1
}
if (-not $esAdmin) {
    $elevatedWindowStyle = if ($ConsoleLog) { "Normal" } else { "Hidden" }
    $elevatedArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$($MyInvocation.MyCommand.Definition)`"")
    if ($ConsoleLog) {
        $elevatedArgs += "-ConsoleLog"
    } else {
        $elevatedArgs += @("-WindowStyle", "Hidden")
    }
    $elevatedProc = Start-Process powershell.exe `
        -ArgumentList ($elevatedArgs -join " ") `
        -Verb RunAs -Wait -PassThru -WindowStyle $elevatedWindowStyle
    $elevatedExit = if ($null -eq $elevatedProc.ExitCode) { 1 } else { [int]$elevatedProc.ExitCode }
    exit $elevatedExit
}

Write-Log "=== INICIO VanySound APO installer ==="
Write-Log "APO LOG: $LOG_FILE"
Write-Log "OS: $([System.Environment]::OSVersion.VersionString) | Arch: $env:PROCESSOR_ARCHITECTURE"

# PASO 1 -- instalar Equalizer APO
$apoYaInstalado = (Test-Path "$apoInstDir\EqualizerAPO.dll") -and (Test-Path "$apoConfig")
if ($apoYaInstalado) {
    Write-Log "EqualizerAPO ya instalado -- saltando NSIS."
} else {
    if (-not (Test-Path $apoExe)) {
        Write-Log "ERROR: No se encontro installer en $apoExe" "ERROR"
        exit 1
    }

    Write-Log "Lanzando NSIS installer silencioso..."
    $nsisProc = Start-Process $apoExe -ArgumentList "/S" -PassThru -WindowStyle Hidden
    $deadline = (Get-Date).AddSeconds(90)
    $killed = 0
    while (-not $nsisProc.HasExited -and (Get-Date) -lt $deadline) {
        $cfg = Get-Process -Name "Configurator" -ErrorAction SilentlyContinue
        if ($cfg) {
            $cfg | Stop-Process -Force -ErrorAction SilentlyContinue
            $killed++
            Write-Log "  Configurator.exe terminado (vez $killed)"
        }
        Start-Sleep -Milliseconds 100
    }
    if (-not $nsisProc.HasExited) { $nsisProc.WaitForExit(30000) }
    Write-Log "NSIS exit code: $($nsisProc.ExitCode) | Configurator matado $killed veces"

    for ($w = 0; $w -lt 10; $w++) {
        if (Test-Path "$apoInstDir\EqualizerAPO.dll") { break }
        Start-Sleep -Seconds 1
    }

    if (-not (Test-Path "$apoInstDir\EqualizerAPO.dll")) {
        Write-Log "CRITICAL: EqualizerAPO.dll no encontrado post-instalacion." "ERROR"
        exit 1
    }
    Write-Log "EqualizerAPO instalado OK."
}

# PASO 2 -- copiar MJUCjr.dll
if (-not (Test-Path $apoVst)) { New-Item $apoVst -ItemType Directory -Force | Out-Null }
$mjucDst = Join-Path $apoVst "MJUCjr.dll"
if (Test-Path $mjucSrc) {
    Copy-Item $mjucSrc $mjucDst -Force
    Write-Log "MJUCjr.dll copiado: $mjucDst"
} else {
    Write-Log "WARN: MJUCjr.dll no encontrado en $mjucSrc" "WARN"
}

# PASO 3 -- helper unificado + bundle cifrado
Install-ControlPlane
Sync-EmbeddedProfilesSource
Ensure-ProfilesBundle
Install-EmbeddedEngineIfAvailable

# PASO 4 -- resolver endpoint objetivo sin depender de Device Selector
Write-Log "Resolviendo endpoint objetivo despues de instalar el control plane..."
$targetEndpoint = $null
for ($i = 0; $i -lt 12; $i++) {
    $targetEndpoint = Ensure-TargetEndpointReady
    if ($targetEndpoint) { break }
    Write-Log "  Intento $($i + 1)/12 -- esperando endpoint Hi-Fi..."
    Start-Sleep -Seconds 2
}

if (-not $targetEndpoint) {
    Write-Log "CRITICAL: No se pudo fijar el endpoint Hi-Fi Cable objetivo ni con helper ni con fallback PnP/registro." "ERROR"
    exit 1
}

$renderGuids = @($targetEndpoint.Guid)
Write-Log "Endpoint objetivo confirmado: $($targetEndpoint.Name) =] $($targetEndpoint.Guid)"
if (-not (Test-DeviceSelectorInstallState -DeviceType "Render" -Guid $targetEndpoint.Guid)) {
    Write-Log "Registrando Equalizer APO directamente sobre el endpoint objetivo..."
    Set-DeviceSelectorInstallState -DeviceType "Render" -Guid $targetEndpoint.Guid -InstallPostMix $true
} else {
    Write-Log "La registracion del APO ya estaba activa sobre el endpoint objetivo."
}

# PASO 5 -- desplegar bundle y aplicar perfil inicial
Deploy-ProfilesBundle
if (Test-Path $installedControlExe) {
    $switchResult = Invoke-ControlCommand -Arguments @("switch", "1") -Prefix "CONTROL SWITCH"
    if ($switchResult.ExitCode -eq 0) {
        Write-Log "Perfil 1 reaplicado tras instalar engine embebido."
    } else {
        Write-Log "WARN: No se pudo reaplicar perfil 1 automaticamente (exit=$($switchResult.ExitCode)). La instalacion continua y la app podra repararlo luego." "WARN"
    }
}

$null = Start-Process "reg.exe" -ArgumentList `
    'add "HKCU\SOFTWARE\EqualizerAPO\Configuration Editor\analysis" /v "resolution" /t REG_DWORD /d 16384 /f' `
    -Wait -WindowStyle Hidden
Write-Log "Resolution APO: 16384"

# PASO 6 -- limpiar distribucion stock + daemon
Apply-DistributionCleanup
Register-ControlDaemon

# PASO 7 -- reiniciar audio
Write-Log "Reiniciando AudioSrv para activar APO..."
try {
    & net stop Audiosrv /y 2>$null
    Start-Sleep -Milliseconds 1000
    & net start Audiosrv 2>$null
    Write-Log "AudioSrv reiniciado OK."
} catch {
    Write-Log "WARN: Error reiniciando AudioSrv (puede requerir reboot): $_" "WARN"
}

Invoke-VerifyOrRepairControlPlane -Reason "post-install"

$apoActivo = (Test-Path "$apoInstDir\EqualizerAPO.dll") -and (Test-Path "$apoConfig\config.txt")
Write-Log "=== FIN | APO instalado=$apoActivo | Dispositivos=$($renderGuids.Count) | ControlPlane=ACTIVO ==="
exit 0
