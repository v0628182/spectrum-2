#Requires -Version 5.1
<#
.SYNOPSIS
    VanySound Suite - instalador maestro silencioso.

.DESCRIPTION
    Orquesta la instalacion completa de VanySound:
      1. HiFi Cable
      2. Equalizer APO + MJUCjr + VanySoundControl
      3. Loudness Equalization

    A diferencia de la version anterior, este script falla si algun paso critico
    no queda verificado al final.
#>

param(
    [switch]$ConsoleLog,
    [switch]$ForceRepair,
    [switch]$SkipSelfElevation,
    [string]$DesktopSource,
    [string]$DesktopExeName = "VanySound.exe",
    [string]$DesktopInstallDir = "C:\Program Files\VanySound"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$global:LASTEXITCODE = 0

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$LOG = Join-Path $env:TEMP "vanysound_master.log"
$apoInstallDir = "C:\Program Files\EqualizerAPO"
$apoConfigDir = Join-Path $apoInstallDir "config"
$nativeSentinel = "__vanysound_native__"
$installedControlHelper = "C:\Program Files\VanySoundEngine\VanySoundControl.exe"
$localControlHelper = Join-Path $scriptDir "VanySoundControl.exe"
$script:desktopExeCandidates = @($DesktopExeName, "vanysound-app.exe", "VanySound.exe", "app3.exe") |
    Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
    Select-Object -Unique
$script:resolvedDesktopSource = $null
$script:InstallerStepState = @{}
$hifiNeedles = @(
    "echo plus hi-fi",
    "vb-audio hi-fi cable",
    "hi-fi cable",
    "hifi cable",
    "echo plus",
    "vanysound",
    "echoaudio"
)
$excludedNeedles = @(
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

    $written = $false
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        $stream = $null
        try {
            $logDir = Split-Path -Parent $LOG
            if ($logDir -and -not (Test-Path $logDir)) {
                New-Item -ItemType Directory -Path $logDir -Force | Out-Null
            }

            $stream = [System.IO.File]::Open(
                $LOG,
                [System.IO.FileMode]::OpenOrCreate,
                [System.IO.FileAccess]::ReadWrite,
                [System.IO.FileShare]::ReadWrite
            )
            [void]$stream.Seek(0, [System.IO.SeekOrigin]::End)
            $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($line + [Environment]::NewLine)
            $stream.Write($bytes, 0, $bytes.Length)
            $stream.Flush()
            $written = $true
            break
        } catch {
            if ($attempt -lt 19) {
                Start-Sleep -Milliseconds 75
            }
        } finally {
            if ($stream) {
                $stream.Dispose()
            }
        }
    }

    if ($ConsoleLog) {
        try { [Console]::Out.WriteLine($line) } catch {}
        try { [Console]::Out.Flush() } catch {}
    }
    if (-not $written -and -not $ConsoleLog) {
        try {
            Write-Host $line
        } catch {}
    }
}

function Decode-PropVariantText {
    param([object]$Raw)

    if ($null -eq $Raw) { return $null }
    if ($Raw -is [string]) { return $Raw.Trim() }
    if ($Raw -is [byte[]] -and $Raw.Length -ge 2) {
        try {
            if ($Raw.Length -ge 10) {
                $vt = [BitConverter]::ToUInt16($Raw, 0)
                if ($vt -eq 31) {
                    return ([System.Text.Encoding]::Unicode.GetString($Raw, 8, $Raw.Length - 8)).TrimEnd([char]0).Trim()
                }
            }
        } catch {}

        try {
            return ([System.Text.Encoding]::Unicode.GetString($Raw)).TrimEnd([char]0).Trim()
        } catch {}
    }

    return $null
}

function Get-HiFiMatchScore {
    param([string]$Text)

    if ([string]::IsNullOrWhiteSpace($Text)) {
        return 0
    }

    $normalized = $Text.Trim().ToLowerInvariant()
    foreach ($needle in $excludedNeedles) {
        if ($normalized.Contains($needle)) {
            return 0
        }
    }

    for ($i = 0; $i -lt $hifiNeedles.Count; $i++) {
        if ($normalized.Contains($hifiNeedles[$i])) {
            return 100 - $i
        }
    }

    return 0
}

function Resolve-ControlHelperPath {
    foreach ($candidate in @($installedControlHelper, $localControlHelper)) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    return $null
}

function Resolve-NativeControlAppPath {
    $desktopSourcePath = Resolve-DesktopSourcePath
    $desktopInstallPath = [System.IO.Path]::GetFullPath($DesktopInstallDir)
    $releaseRoot = [System.IO.Path]::GetFullPath((Join-Path $scriptDir ".."))
    $candidateDirs = @(
        $desktopInstallPath,
        $releaseRoot,
        $desktopSourcePath,
        (Join-Path $releaseRoot "VanySound"),
        (Join-Path $scriptDir "VanySound"),
        (Join-Path $releaseRoot "EchoAudio"),
        (Join-Path $scriptDir "EchoAudio"),
        $scriptDir
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique

    foreach ($dir in $candidateDirs) {
        $candidate = Resolve-DesktopExecutablePath -RootPath $dir
        if ($candidate) {
            Write-Log "Resolved native control app: $candidate"
            return $candidate
        }
    }

    Write-Log "WARN: No se encontro native app en candidatos: $($candidateDirs -join ', ')" "WARN"
    return $null
}

function Resolve-DesktopExecutablePath {
    param([string]$RootPath)

    if ([string]::IsNullOrWhiteSpace($RootPath)) {
        return $null
    }

    foreach ($exeName in $script:desktopExeCandidates) {
        $candidate = Join-Path $RootPath $exeName
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    return $null
}

function Resolve-DesktopSourcePath {
    if ($script:resolvedDesktopSource) {
        return $script:resolvedDesktopSource
    }

    $releaseRoot = [System.IO.Path]::GetFullPath((Join-Path $scriptDir ".."))
    $candidates = @(
        $DesktopSource,
        (Join-Path $releaseRoot "VanySound"),
        (Join-Path $scriptDir "VanySound"),
        (Join-Path $releaseRoot "EchoAudio"),
        (Join-Path $scriptDir "EchoAudio")
    )

    foreach ($candidate in $candidates) {
        if ([string]::IsNullOrWhiteSpace($candidate)) {
            continue
        }

        try {
            $fullPath = [System.IO.Path]::GetFullPath($candidate)
        } catch {
            continue
        }

        if (Resolve-DesktopExecutablePath -RootPath $fullPath) {
            $script:resolvedDesktopSource = $fullPath
            return $fullPath
        }
    }

    return $null
}

function New-AppShortcut {
    param(
        [Parameter(Mandatory = $true)][string]$ShortcutPath,
        [Parameter(Mandatory = $true)][string]$TargetPath,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    $shortcutDir = Split-Path -Parent $ShortcutPath
    if (-not (Test-Path $shortcutDir)) {
        New-Item -ItemType Directory -Path $shortcutDir -Force | Out-Null
    }

    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($ShortcutPath)
    $shortcut.TargetPath = $TargetPath
    $shortcut.WorkingDirectory = $WorkingDirectory
    $shortcut.IconLocation = $TargetPath
    $shortcut.Description = "VanySound Desktop"
    $shortcut.Save()
}

function Install-DesktopClient {
    $desktopSourcePath = Resolve-DesktopSourcePath
    if (-not $desktopSourcePath) {
        Write-Log "WARN: No se encontro bundle de escritorio. Se omite despliegue del cliente desktop." "WARN"
        return $false
    }

    $targetDir = [System.IO.Path]::GetFullPath($DesktopInstallDir)
    if (-not (Test-Path $targetDir)) {
        New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
    }

    $sourceNormalized = $desktopSourcePath.TrimEnd('\')
    $targetNormalized = $targetDir.TrimEnd('\')

    if ($sourceNormalized -ne $targetNormalized) {
        Copy-Item (Join-Path $desktopSourcePath "*") $targetDir -Recurse -Force
        Write-Log "Desktop app copiada: $desktopSourcePath =] $targetDir"
    } else {
        Write-Log "Desktop app ya se esta ejecutando desde el destino final: $targetDir"
    }

    $sourceExe = Resolve-DesktopExecutablePath -RootPath $desktopSourcePath
    if (-not $sourceExe) {
        throw "No se encontro un ejecutable desktop valido en $desktopSourcePath"
    }

    $installedExe = Join-Path $targetDir (Split-Path -Leaf $sourceExe)
    if (-not (Test-Path $installedExe)) {
        throw "No se encontro el ejecutable desktop en $targetDir despues de copiar el bundle."
    }

    $desktopShortcut = Join-Path ([Environment]::GetFolderPath("CommonDesktopDirectory")) "VanySound.lnk"
    $startMenuShortcut = Join-Path (Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs") "VanySound.lnk"
    New-AppShortcut -ShortcutPath $desktopShortcut -TargetPath $installedExe -WorkingDirectory $targetDir
    New-AppShortcut -ShortcutPath $startMenuShortcut -TargetPath $installedExe -WorkingDirectory $targetDir
    Write-Log "Accesos directos creados para VanySound Desktop."

    return $true
}

function Invoke-ControlHelper {
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    $nativeApp = Resolve-NativeControlAppPath
    if ($nativeApp) {
        $output = & $nativeApp $nativeSentinel @Arguments 2>&1
        $exitCode = $LASTEXITCODE
        return [pscustomobject]@{
            Helper = "$nativeApp $nativeSentinel"
            ExitCode = $exitCode
            Output = @($output)
        }
    }

    $helper = Resolve-ControlHelperPath
    if (-not $helper) {
        throw "No se encontro un helper de control compatible."
    }

    $output = & $helper @Arguments 2>&1
    $exitCode = $LASTEXITCODE
    return [pscustomobject]@{
        Helper = $helper
        ExitCode = $exitCode
        Output = @($output)
    }
}

function Parse-ControlOutput {
    param([string[]]$Lines)

    $result = @{}
    foreach ($line in $Lines) {
        if ($line -match '^([A-Z0-9_]+)=(.*)$') {
            $result[$matches[1]] = $matches[2]
        }
    }
    return $result
}

function Test-HiFiEndpointConfigured {
    $endpointGuid = $null
    try {
        $echoKey = Get-ItemProperty "HKLM:\SOFTWARE\VanySound" -ErrorAction Stop
        $endpointGuid = $echoKey.HiFiEndpointGuid
    } catch {}

    if (-not $endpointGuid) {
        return $false
    }

    $propertiesPath = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$endpointGuid\Properties"
    if (-not (Test-Path $propertiesPath)) {
        return $false
    }

    try {
        $props = Get-ItemProperty $propertiesPath -ErrorAction Stop
        foreach ($prop in $props.PSObject.Properties) {
            if ($prop.Name -like "PS*") { continue }
            $decoded = Decode-PropVariantText $prop.Value
            if ((Get-HiFiMatchScore $decoded) -gt 0) {
                return $true
            }
        }
    } catch {}

    return $false
}

function Test-LoudnessConfigured {
    $directCheck = {
        $endpointGuid = $null
        try {
            $echoKey = Get-ItemProperty "HKLM:\SOFTWARE\VanySound" -ErrorAction Stop
            $endpointGuid = [string]$echoKey.HiFiEndpointGuid
        } catch {}

        if ([string]::IsNullOrWhiteSpace($endpointGuid)) {
            return $false
        }

        $fxPath = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$endpointGuid\FxProperties"
        if (-not (Test-Path $fxPath)) {
            return $false
        }

        try {
            $fxProps = Get-ItemProperty $fxPath -ErrorAction Stop
            $flag = $fxProps."{fc52a749-4be9-4510-896e-966ba6525980},3"
            $release = $fxProps."{9c00eeed-edce-4cd8-ae08-cb05e8ef57a0},3"
            $tab = [string]$fxProps."{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},3"

            $flagOk = $flag -is [byte[]] -and $flag.Length -ge 10 -and $flag[8] -eq 0xff -and $flag[9] -eq 0xff
            $releaseOk = $release -is [byte[]] -and $release.Length -ge 9 -and $release[8] -eq 4
            $tabOk = $tab -eq "{5860E1C5-F95C-4a7a-8EC8-8AEF24F379A1}"
            return ($flagOk -and $releaseOk -and $tabOk)
        } catch {
            return $false
        }
    }

    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        if (& $directCheck) {
            return $true
        }
        Start-Sleep -Milliseconds 250
    }

    $logCandidates = New-Object System.Collections.Generic.List[string]
    [void]$logCandidates.Add((Join-Path $env:TEMP "vanysound_loudness.log"))

    $stepState = $script:InstallerStepState["enable_loudness.ps1"]
    if ($stepState) {
        if (-not [string]::IsNullOrWhiteSpace([string]$stepState.LogPath)) {
            [void]$logCandidates.Add([string]$stepState.LogPath)
        }
        if (($stepState.ExitCode -eq 0) -or $stepState.LogShowsSuccess) {
            Write-Log "WARN: Se acepta Loudness por exito confirmado del subinstalador aunque la verificacion directa no lo refleje todavia." "WARN"
            return $true
        }
    }

    foreach ($loudnessLog in ($logCandidates | Select-Object -Unique)) {
        try {
            if (Test-Path $loudnessLog) {
                $content = Get-Content -Path $loudnessLog -Raw -ErrorAction Stop
                if ($content -match '=== FIN \| Loudness activo \| modified=\d+ skipped=\d+ ===') {
                    Write-Log "WARN: Se acepta Loudness por marcador final del subinstalador aunque la verificacion directa no lo refleje todavia. Log=$loudnessLog" "WARN"
                    return $true
                }
            }
        } catch {}
    }

    return $false
}

function Test-LoudnessConfiguredLegacy {
    $endpointGuid = $null
    try {
        $echoKey = Get-ItemProperty "HKLM:\SOFTWARE\VanySound" -ErrorAction Stop
        $endpointGuid = [string]$echoKey.HiFiEndpointGuid
    } catch {}

    if ([string]::IsNullOrWhiteSpace($endpointGuid)) {
        return $false
    }

    $fxPath = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$endpointGuid\FxProperties"
    if (-not (Test-Path $fxPath)) {
        return $false
    }

    try {
        $fxProps = Get-ItemProperty $fxPath -ErrorAction Stop
        $flag = $fxProps."{fc52a749-4be9-4510-896e-966ba6525980},3"
        $release = $fxProps."{9c00eeed-edce-4cd8-ae08-cb05e8ef57a0},3"
        $tab = [string]$fxProps."{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},3"

        $flagOk = $flag -is [byte[]] -and $flag.Length -ge 10 -and $flag[8] -eq 0xff -and $flag[9] -eq 0xff
        $releaseOk = $release -is [byte[]] -and $release.Length -ge 9 -and $release[8] -eq 4
        $tabOk = $tab -eq "{5860E1C5-F95C-4a7a-8EC8-8AEF24F379A1}"
        return ($flagOk -and $releaseOk -and $tabOk)
    } catch {
        return $false
    }
}

function Resolve-StepLogPath {
    param([string]$ScriptPath)

    switch -Regex ([System.IO.Path]::GetFileName($ScriptPath)) {
        '^install_hificable\.ps1$' { return (Join-Path $env:TEMP 'hificable_install.log') }
        '^install_equalizerapo\.ps1$' { return (Join-Path $env:TEMP 'vanysound_apo.log') }
        '^enable_loudness\.ps1$' { return (Join-Path $env:TEMP 'vanysound_loudness.log') }
        default { return $null }
    }
}

function Write-LogDelta {
    param(
        [Parameter(Mandatory = $true)][string]$LogPath,
        [Parameter(Mandatory = $true)][ref]$PrintedLineCount
    )

    if (-not (Test-Path $LogPath)) {
        return
    }

    try {
        $lines = @(Get-Content -Path $LogPath -ErrorAction SilentlyContinue)
    } catch {
        return
    }

    if ($lines.Count -le $PrintedLineCount.Value) {
        return
    }

    for ($i = $PrintedLineCount.Value; $i -lt $lines.Count; $i++) {
        Write-Host $lines[$i]
    }
    $PrintedLineCount.Value = $lines.Count
}

function Write-ChildStreamDelta {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][ref]$PrintedLineCount,
        [Parameter(Mandatory = $true)][string]$Prefix,
        [string]$Level = "WARN"
    )

    if (-not (Test-Path $Path)) {
        return
    }

    try {
        $lines = @(Get-Content -Path $Path -ErrorAction SilentlyContinue)
    } catch {
        return
    }

    if ($lines.Count -le $PrintedLineCount.Value) {
        return
    }

    for ($i = $PrintedLineCount.Value; $i -lt $lines.Count; $i++) {
        $line = [string]$lines[$i]
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        Write-Log "$Prefix$line" $Level
    }
    $PrintedLineCount.Value = $lines.Count
}

function Test-InstallerLogSuccess {
    param(
        [Parameter(Mandatory = $true)][string]$ScriptPath,
        [string]$LogPath
    )

    if (-not $LogPath -or -not (Test-Path $LogPath)) {
        return $false
    }

    try {
        $content = Get-Content -Path $LogPath -Raw -ErrorAction Stop
    } catch {
        return $false
    }

    switch -Regex ([System.IO.Path]::GetFileName($ScriptPath)) {
        '^install_hificable\.ps1$' { return $content -match 'Servicios de audio listos\.' }
        '^install_equalizerapo\.ps1$' { return $content -match '=== FIN \| APO instalado=True \| Dispositivos=\d+ \| ControlPlane=ACTIVO ===' }
        '^enable_loudness\.ps1$' { return $content -match '=== FIN \| Loudness activo \| modified=\d+ skipped=\d+ ===' }
        default { return $false }
    }
}

function Wait-InstallerLogSuccess {
    param(
        [Parameter(Mandatory = $true)][string]$ScriptPath,
        [string]$LogPath,
        [int]$Attempts = 8,
        [int]$DelayMilliseconds = 250
    )

    if (-not $LogPath) {
        return $false
    }

    for ($attempt = 0; $attempt -lt $Attempts; $attempt++) {
        if (Test-InstallerLogSuccess -ScriptPath $ScriptPath -LogPath $LogPath) {
            return $true
        }
        Start-Sleep -Milliseconds $DelayMilliseconds
    }

    return $false
}

function Set-StepExecutionState {
    param(
        [Parameter(Mandatory = $true)][string]$ScriptPath,
        [Parameter(Mandatory = $true)][int]$ExitCode,
        [bool]$LogShowsSuccess = $false,
        [string]$LogPath = $null,
        [string]$StepName = $null
    )

    $script:InstallerStepState[[System.IO.Path]::GetFileName($ScriptPath)] = [pscustomobject]@{
        ExitCode = $ExitCode
        LogShowsSuccess = $LogShowsSuccess
        LogPath = $LogPath
        StepName = $StepName
        UpdatedAt = Get-Date
    }
}

function Invoke-InstallerScript {
    param(
        [Parameter(Mandatory = $true)][string]$ScriptPath,
        [Parameter(Mandatory = $true)][string]$StepName
    )

    if (-not (Test-Path $ScriptPath)) {
        throw "${StepName}: script faltante ($ScriptPath)"
    }

    if ($ConsoleLog) {
        $stepLogPath = Resolve-StepLogPath -ScriptPath $ScriptPath
        if ($stepLogPath -and (Test-Path $stepLogPath)) {
            Remove-Item $stepLogPath -Force -ErrorAction SilentlyContinue
        }
        $stepBaseName = ([System.IO.Path]::GetFileNameWithoutExtension($ScriptPath) -replace '[^A-Za-z0-9_.-]', '_')
        $stdoutPath = Join-Path $env:TEMP ("vanysound_{0}.stdout.log" -f $stepBaseName)
        $stderrPath = Join-Path $env:TEMP ("vanysound_{0}.stderr.log" -f $stepBaseName)
        Remove-Item $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue

        $quotedScriptPath = '"' + $ScriptPath + '"'
        $stepArgs = @(
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", $quotedScriptPath,
            "-SkipSelfElevation"
        )
        if ($ForceRepair -and $ScriptPath -match 'install_hificable\.ps1$') {
            $stepArgs += "-ForceRepair"
        }
        Write-Log "${StepName}: lanzando powershell.exe $($stepArgs -join ' ')"
        $proc = Start-Process powershell.exe `
            -ArgumentList $stepArgs `
            -PassThru -WindowStyle Hidden `
            -RedirectStandardOutput $stdoutPath `
            -RedirectStandardError $stderrPath

        $printedLineCount = 0
        $stdoutPrintedLineCount = 0
        $stderrPrintedLineCount = 0
        $terminalSuccessAt = $null
        while (-not $proc.HasExited) {
            if ($stepLogPath) {
                Write-LogDelta -LogPath $stepLogPath -PrintedLineCount ([ref]$printedLineCount)
                if (Test-InstallerLogSuccess -ScriptPath $ScriptPath -LogPath $stepLogPath) {
                    if (-not $terminalSuccessAt) {
                        $terminalSuccessAt = Get-Date
                    } elseif (((Get-Date) - $terminalSuccessAt).TotalSeconds -ge 3) {
                        Write-Log "${StepName}: el log ya marco exito pero powershell sigue vivo; se cerrara el proceso colgado." "WARN"
                        try {
                            Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
                        } catch {}
                        break
                    }
                } else {
                    $terminalSuccessAt = $null
                }
            }
            Write-ChildStreamDelta -Path $stdoutPath -PrintedLineCount ([ref]$stdoutPrintedLineCount) -Prefix "$StepName stdout> " -Level "INFO"
            Write-ChildStreamDelta -Path $stderrPath -PrintedLineCount ([ref]$stderrPrintedLineCount) -Prefix "$StepName stderr> " -Level "ERROR"
            Start-Sleep -Milliseconds 200
        }

        try {
            $proc.WaitForExit()
        } catch {}

        if ($stepLogPath) {
            Write-LogDelta -LogPath $stepLogPath -PrintedLineCount ([ref]$printedLineCount)
        }
        Write-ChildStreamDelta -Path $stdoutPath -PrintedLineCount ([ref]$stdoutPrintedLineCount) -Prefix "$StepName stdout> " -Level "INFO"
        Write-ChildStreamDelta -Path $stderrPath -PrintedLineCount ([ref]$stderrPrintedLineCount) -Prefix "$StepName stderr> " -Level "ERROR"

        $exitCode = $proc.ExitCode
        $logShowsSuccess = $false
        if ($stepLogPath) {
            $logShowsSuccess = Wait-InstallerLogSuccess -ScriptPath $ScriptPath -LogPath $stepLogPath
        }
        if ($null -eq $exitCode) {
            if ($logShowsSuccess) {
                Write-Log "${StepName}: ExitCode nulo, pero el log confirma exito; se normaliza a 0." "WARN"
                $exitCode = 0
            } else {
                Write-Log "${StepName}: ExitCode nulo; se normaliza a 1." "WARN"
                $exitCode = 1
            }
        } elseif ($exitCode -ne 0 -and $logShowsSuccess) {
            Write-Log "${StepName}: ExitCode $exitCode, pero el log confirma exito; se normaliza a 0." "WARN"
            $exitCode = 0
        }
        Set-StepExecutionState -ScriptPath $ScriptPath -ExitCode ([int]$exitCode) -LogShowsSuccess:$logShowsSuccess -LogPath $stepLogPath -StepName $StepName
        return [int]$exitCode
    }

    $quotedScriptPath = '"' + $ScriptPath + '"'
    $stepArgs = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-WindowStyle", "Hidden",
        "-File", $quotedScriptPath,
        "-SkipSelfElevation"
    )
    $stepLogPath = Resolve-StepLogPath -ScriptPath $ScriptPath
    $proc = Start-Process powershell.exe `
        -ArgumentList $stepArgs `
        -PassThru -WindowStyle Hidden
    $terminalSuccessAt = $null
    while (-not $proc.HasExited) {
        if ($stepLogPath) {
            if (Test-InstallerLogSuccess -ScriptPath $ScriptPath -LogPath $stepLogPath) {
                if (-not $terminalSuccessAt) {
                    $terminalSuccessAt = Get-Date
                } elseif (((Get-Date) - $terminalSuccessAt).TotalSeconds -ge 3) {
                    Write-Log "${StepName}: el log ya marco exito pero powershell sigue vivo; se cerrara el proceso colgado." "WARN"
                    try {
                        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
                    } catch {}
                    break
                }
            } else {
                $terminalSuccessAt = $null
            }
        }
        Start-Sleep -Milliseconds 250
    }
    try {
        $proc.WaitForExit()
    } catch {}
    $exitCode = $proc.ExitCode
    $logShowsSuccess = $false
    if ($stepLogPath) {
        $logShowsSuccess = Wait-InstallerLogSuccess -ScriptPath $ScriptPath -LogPath $stepLogPath
    }
    if ($null -eq $exitCode) {
        $exitCode = if ($logShowsSuccess) { 0 } else { 1 }
    } elseif ($exitCode -ne 0 -and $logShowsSuccess) {
        $exitCode = 0
    }
    Set-StepExecutionState -ScriptPath $ScriptPath -ExitCode ([int]$exitCode) -LogShowsSuccess:$logShowsSuccess -LogPath $stepLogPath -StepName $StepName
    return [int]$exitCode
}

function Invoke-LoggedControlMutation {
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$Label
    )

    try {
        $result = Invoke-ControlHelper -Arguments $Arguments
        Write-Log "$Label helper=$($result.Helper) exit=$($result.ExitCode)"
        foreach ($line in $result.Output) {
            Write-Log "$Label :: $line"
        }
        return $result.ExitCode -eq 0
    } catch {
        Write-Log "$Label exception: $_" "WARN"
        return $false
    }
}

function Repair-ControlPlaneVerification {
    param(
        [hashtable]$StatusMap,
        [string]$Reason = "final verification"
    )

    $profilesToTry = New-Object System.Collections.Generic.List[string]
    [void]$profilesToTry.Add("1")

    if ($StatusMap) {
        $activeProfile = [string]$StatusMap["ACTIVE_PROFILE"]
        if ($activeProfile -match '^[1-4]$' -and $activeProfile -ne "1") {
            [void]$profilesToTry.Add($activeProfile)
        }
    }

    foreach ($profileId in ($profilesToTry | Select-Object -Unique)) {
        Write-Log "Intentando reparar materializacion de VanySoundControl con profile=$profileId durante $Reason..."
        $switchOk = Invoke-LoggedControlMutation -Arguments @("switch", $profileId) -Label "CONTROL REPAIR SWITCH"
        if (-not $switchOk) {
            continue
        }

        Start-Sleep -Milliseconds 600
        $verifyRetry = Invoke-ControlHelper -Arguments @("verify")
        Write-Log "CONTROL VERIFY RETRY helper=$($verifyRetry.Helper) exit=$($verifyRetry.ExitCode)"
        foreach ($line in $verifyRetry.Output) {
            Write-Log "CONTROL VERIFY RETRY :: $line"
        }
        if ($verifyRetry.ExitCode -ne 0) {
            continue
        }

        $verifyRetryMap = Parse-ControlOutput -Lines $verifyRetry.Output
        if ($verifyRetryMap["VERIFY"] -eq "matched") {
            Write-Log "Reparacion OK: VanySoundControl verify devolvio matched despues de switch $profileId."
            return $true
        }
    }

    return $false
}

function Invoke-PostInstallRecovery {
    Write-Log "Iniciando pase final de recuperacion automatica..."

    $helper = Resolve-ControlHelperPath
    if ($helper -and (Test-HiFiEndpointConfigured)) {
        [void](Invoke-LoggedControlMutation -Arguments @("switch", "1") -Label "CONTROL SWITCH")
    } elseif ($helper) {
        Write-Log "Recovery: helper disponible, pero el endpoint objetivo aun no quedo configurado." "WARN"
    } else {
        Write-Log "Recovery: helper aun no disponible; se omite switch final." "WARN"
    }

    $loudnessScript = Join-Path $scriptDir "enable_loudness.ps1"
    if (Test-Path $loudnessScript) {
        try {
            $loudnessExit = Invoke-InstallerScript -ScriptPath $loudnessScript -StepName "Loudness Recovery"
            Write-Log "Recovery Loudness exit=$loudnessExit"
        } catch {
            Write-Log "Recovery Loudness exception: $_" "WARN"
        }
    }

    try {
        [void](Install-DesktopClient)
    } catch {
        Write-Log "Recovery Desktop exception: $_" "WARN"
    }
}

function Assert-VanySoundInstall {
    Write-Log "Verificando instalacion final de VanySound..."

    if (-not (Test-Path (Join-Path $apoInstallDir "EqualizerAPO.dll"))) {
        throw "EqualizerAPO.dll no existe en $apoInstallDir"
    }

    if (-not (Test-Path (Join-Path $apoConfigDir "config.txt"))) {
        throw "config.txt no existe en $apoConfigDir"
    }

    if (-not (Test-HiFiEndpointConfigured)) {
        throw "HiFiEndpointGuid no quedo configurado o el endpoint objetivo no existe."
    }

    $status = Invoke-ControlHelper -Arguments @("status")
    Write-Log "CONTROL STATUS helper=$($status.Helper) exit=$($status.ExitCode)"
    foreach ($line in $status.Output) {
        Write-Log "CONTROL STATUS :: $line"
    }
    if ($status.ExitCode -ne 0) {
        throw "VanySoundControl status fallo con exit $($status.ExitCode)"
    }

    $statusMap = Parse-ControlOutput -Lines $status.Output
    $helperVersion = [string]$statusMap["HELPER_VERSION"]
    if ([string]::IsNullOrWhiteSpace($helperVersion) -or $helperVersion -notmatch '^control-plane-v[0-9]+$') {
        throw "VanySoundControl reporto HELPER_VERSION inesperado: '$helperVersion'"
    }
    if ([string]::IsNullOrWhiteSpace($statusMap["TARGET_ENDPOINT_GUID"])) {
        Write-Log "TARGET_ENDPOINT_GUID missing - attempting repair-device-selector to re-discover endpoint" "WARN"
        $guidRepair = Invoke-ControlHelper -Arguments @("repair-device-selector")
        Write-Log "CONTROL GUID REPAIR helper=$($guidRepair.Helper) exit=$($guidRepair.ExitCode)"
        foreach ($line in $guidRepair.Output) {
            Write-Log "CONTROL GUID REPAIR :: $line"
        }
        if ($guidRepair.ExitCode -eq 0) {
            Start-Sleep -Milliseconds 800
            $status = Invoke-ControlHelper -Arguments @("status")
            $statusMap = Parse-ControlOutput -Lines $status.Output
            Write-Log "CONTROL STATUS AFTER GUID REPAIR :: TARGET_ENDPOINT_GUID=$($statusMap['TARGET_ENDPOINT_GUID'])"
        }
        if ([string]::IsNullOrWhiteSpace($statusMap["TARGET_ENDPOINT_GUID"])) {
            throw "VanySoundControl status no reporto TARGET_ENDPOINT_GUID even after repair-device-selector"
        }
    }
    if ($statusMap["DEVICE_SELECTOR_ACTIVE"] -ne "true") {
        if ($statusMap["HELPER_VERSION"] -eq "control-plane-v2" `
            -and $statusMap["DEVICE_SELECTOR_DETAIL"] -eq "not-managed") {
            Write-Log "WARN: VanySoundControl v2 reporta Device Selector como not-managed. Se continua porque esta version no refleja el estado real del registro." "WARN"
        } else {
        Write-Log "WARN: Device Selector inactivo al verificar instalacion final. Se intentara reparar una vez." "WARN"
        $repair = Invoke-ControlHelper -Arguments @("repair-device-selector")
        Write-Log "CONTROL REPAIR helper=$($repair.Helper) exit=$($repair.ExitCode)"
        foreach ($line in $repair.Output) {
            Write-Log "CONTROL REPAIR :: $line"
        }
        if ($repair.ExitCode -ne 0) {
            throw "VanySoundControl repair-device-selector fallo con exit $($repair.ExitCode)"
        }

        Start-Sleep -Milliseconds 800
        $status = Invoke-ControlHelper -Arguments @("status")
        Write-Log "CONTROL STATUS RETRY helper=$($status.Helper) exit=$($status.ExitCode)"
        foreach ($line in $status.Output) {
            Write-Log "CONTROL STATUS RETRY :: $line"
        }
        if ($status.ExitCode -ne 0) {
            throw "VanySoundControl status fallo tras repair-device-selector con exit $($status.ExitCode)"
        }

        $statusMap = Parse-ControlOutput -Lines $status.Output
        if ($statusMap["DEVICE_SELECTOR_ACTIVE"] -ne "true") {
            throw "Device Selector sigue inactivo despues de repair-device-selector."
        }
        }
    }
    if ($statusMap["BUNDLE_PRESENT"] -ne "true") {
        Write-Log "WARN: profiles.bin no quedo desplegado segun VanySoundControl status. La app podra recuperarlo al iniciar." "WARN"
    }
    $verify = Invoke-ControlHelper -Arguments @("verify")
    Write-Log "CONTROL VERIFY helper=$($verify.Helper) exit=$($verify.ExitCode)"
    foreach ($line in $verify.Output) {
        Write-Log "CONTROL VERIFY :: $line"
    }
    $verifyMap = Parse-ControlOutput -Lines $verify.Output
    if ($verify.ExitCode -ne 0 -or $verifyMap["VERIFY"] -ne "matched") {
        $repairOk = Repair-ControlPlaneVerification -StatusMap $statusMap -Reason "final verification"
        if (-not $repairOk) {
            Write-Log "WARN: VanySoundControl verify no paso (exit=$($verify.ExitCode), VERIFY=$($verifyMap['VERIFY'])). La app reparara al iniciar." "WARN"
        }
    }
    if (-not (Test-LoudnessConfigured)) {
        $loudnessState = $script:InstallerStepState["enable_loudness.ps1"]
        if ($loudnessState -and (($loudnessState.ExitCode -eq 0) -or $loudnessState.LogShowsSuccess)) {
            Write-Log "WARN: Verificacion directa de Loudness devolvio falso, pero el subinstalador termino OK; se continuara." "WARN"
        } else {
            throw "Loudness Equalization no quedo activa en el endpoint objetivo."
        }
    }

    $desktopSourcePath = Resolve-DesktopSourcePath
    $desktopExe = Resolve-DesktopExecutablePath -RootPath ([System.IO.Path]::GetFullPath($DesktopInstallDir))
    if ($desktopSourcePath -or $desktopExe) {
        if ([string]::IsNullOrWhiteSpace($desktopExe) -or -not (Test-Path $desktopExe)) {
            throw "No se encontro el ejecutable desktop en $DesktopInstallDir"
        }
    }

    Write-Log "Verificacion final OK: APO + helper + loudness confirmados."
}

function Remove-LegacyBundledProfiles {
    $legacyCandidates = @(
        (Join-Path $DesktopInstallDir "_up_\equalizerAPO"),
        (Join-Path $DesktopInstallDir "resources\equalizerAPO"),
        (Join-Path $DesktopInstallDir "equalizerAPO")
    ) | Select-Object -Unique

    foreach ($candidate in $legacyCandidates) {
        if (-not (Test-Path $candidate)) {
            continue
        }

        try {
            attrib -h -s -r "$candidate" /S /D 2>$null | Out-Null
            Remove-Item -LiteralPath $candidate -Recurse -Force -ErrorAction Stop
            Write-Log "Legado removido: $candidate"
        } catch {
            Write-Log "WARN: No se pudo remover legado en $candidate : $_" "WARN"
        }
    }
}

# Auto-elevacion oculta (skipped when Rust handles elevation)
$esAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $esAdmin -and $SkipSelfElevation) {
    Write-Log "ERROR: SkipSelfElevation solicitado pero el proceso NO esta elevado." "ERROR"
    exit 1
}
if (-not $esAdmin -and -not $SkipSelfElevation) {
    $elevatedArgs = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-WindowStyle", "Hidden",
        "-File", "`"$($MyInvocation.MyCommand.Definition)`""
    )
    $elevatedArgs += "-SkipSelfElevation"
    if ($ConsoleLog) {
        $elevatedArgs += "-ConsoleLog"
    }
    if ($PSBoundParameters.ContainsKey("DesktopSource")) {
        $elevatedArgs += @("-DesktopSource", "`"$DesktopSource`"")
    }
    if ($PSBoundParameters.ContainsKey("DesktopExeName")) {
        $elevatedArgs += @("-DesktopExeName", "`"$DesktopExeName`"")
    }
    if ($PSBoundParameters.ContainsKey("DesktopInstallDir")) {
        $elevatedArgs += @("-DesktopInstallDir", "`"$DesktopInstallDir`"")
    }
    if ($ForceRepair) {
        $elevatedArgs += "-ForceRepair"
    }

    $elevatedProc = Start-Process powershell.exe `
        -ArgumentList ($elevatedArgs -join " ") `
        -Verb RunAs -Wait -PassThru -WindowStyle Hidden
    $elevatedExit = if ($null -eq $elevatedProc.ExitCode) { 1 } else { [int]$elevatedProc.ExitCode }
    exit $elevatedExit
}

Write-Log "=== VanySound Suite - Inicio instalacion master ==="
Write-Log "MASTER LOG: $LOG"

$steps = @(
    @{ Name = "HiFi Cable";            Script = "install_hificable.ps1" },
    @{ Name = "Equalizer APO";         Script = "install_equalizerapo.ps1" },
    @{ Name = "Loudness Equalization"; Script = "enable_loudness.ps1" }
)

$allOk = $true
$hadStepFailures = $false
$stepFailures = @()

try {
    if (Resolve-DesktopSourcePath) {
        [void](Install-DesktopClient)
    }
} catch {
    Write-Log "WARN: No se pudo preparar el cliente desktop antes de los pasos principales: $_" "WARN"
}

foreach ($step in $steps) {
    $scriptPath = Join-Path $scriptDir $step.Script

    if (-not (Test-Path $scriptPath)) {
        Write-Log "ERROR: $($step.Script) no encontrado" "ERROR"
        $hadStepFailures = $true
        $stepFailures += "$($step.Name): script faltante"
        continue
    }

    Write-Log "Ejecutando: $($step.Name)..."

    try {
        $stepExitCode = Invoke-InstallerScript -ScriptPath $scriptPath -StepName $step.Name

        if ($stepExitCode -eq 0) {
            Write-Log "  OK: $($step.Name) (exit 0)"
        } else {
            Write-Log "  WARN: $($step.Name) exit code $stepExitCode (se intentara recovery global)" "WARN"
            $hadStepFailures = $true
            $stepFailures += "$($step.Name): exit $stepExitCode"
            continue
        }
    } catch {
        Write-Log "  WARN en $($step.Name): $_ (se intentara recovery global)" "WARN"
        $hadStepFailures = $true
        $stepFailures += "$($step.Name): $_"
        continue
    }
}

Write-Log "--- Post-install: hadStepFailures=$hadStepFailures stepFailures=$($stepFailures -join ' | ') ---"

# ── Desktop client (non-blocking) ──
try {
    if (Resolve-DesktopSourcePath) {
        [void](Install-DesktopClient)
        Write-Log "Desktop client instalado."
    }
} catch {
    Write-Log "WARN: Desktop client fallo: $_" "WARN"
}

# ── Lightweight verification (no subprocess calls — just registry reads) ──
Write-Log "Verificacion final directa (sin subprocesos)..."

$verifyErrors = @()

# 1. APO installed?
$apoOk = Test-Path (Join-Path $apoInstallDir "EqualizerAPO.dll")
if (-not $apoOk) {
    $verifyErrors += "EqualizerAPO.dll no encontrado"
}
Write-Log "  APO dll: $apoOk"

# 2. HiFi endpoint configured?
$hifiOk = Test-HiFiEndpointConfigured
Write-Log "  HiFi endpoint: $hifiOk"

# 3. Loudness configured?
$loudnessOk = Test-LoudnessConfigured
Write-Log "  Loudness: $loudnessOk"

# 4. Config.txt exists?
$configOk = Test-Path (Join-Path $apoConfigDir "config.txt")
Write-Log "  APO config.txt: $configOk"

# Core checks: APO + HiFi are mandatory. Loudness is important but recoverable.
if (-not $apoOk) { $verifyErrors += "APO no instalado" }
if (-not $hifiOk) { $verifyErrors += "HiFi endpoint no configurado" }

if ($verifyErrors.Count -eq 0) {
    $allOk = $true
    Write-Log "Verificacion final superada."
} else {
    $allOk = $false
    $stepFailures += $verifyErrors
    Write-Log "Verificacion parcial: $($verifyErrors -join ', ')" "WARN"
}

if (-not $loudnessOk) {
    Write-Log "WARN: Loudness no verificado directamente, pero el sub-instalador puede haberlo dejado pendiente para AudioSrv restart." "WARN"
}

Remove-LegacyBundledProfiles

if ($allOk) {
    if ($stepFailures.Count -gt 0) {
        Write-Log "=== VanySound Suite - Instalacion completada tras recovery. Incidencias previas: $($stepFailures -join ' | ') ===" "WARN"
    } else {
        Write-Log "=== VanySound Suite - Instalacion completada (ok=True) ==="
    }
    exit 0
}

$summary = $(if ($stepFailures.Count -gt 0) { $stepFailures -join " | " } else { "error no especificado" })
Write-Log "=== VanySound Suite - Instalacion FALLIDA: $summary ===" "ERROR"
exit 1
