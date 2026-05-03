#Requires -Version 5.1
<#
.SYNOPSIS
    Instalación 100% silenciosa de HiFi Cable & ASIO Bridge (VB-Audio)

.DESCRIPTION
    INGENIERÍA INVERSA: Instala el driver directamente usando SetupAPI (setupapi.dll + newdev.dll)
    sin ejecutar el instalador original. Sin ventanas, sin GUI, sin coordenadas.
    Compatible: Windows 10/11 (x64).

.ESTRUCTURA
    instalacion\
      install_hificable.ps1   <- este script
      driver\
        vbhfvaio64_win7.inf
        vbaudio_hfvaio64_win7.sys
        vbaudio_hfvaio64_win7.cat
#>

param(
    [switch]$ConsoleLog,
    [switch]$SkipSelfElevation,
    [switch]$ForceRepair
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "SilentlyContinue"

$HARDWARE_ID  = "VBAudioHFVAIO"
$MEDIA_GUID   = "{4d36e96c-e325-11ce-bfc1-08002be10318}"
$LOG_FILE     = Join-Path $env:TEMP "hificable_install.log"
$scriptDir    = Split-Path -Parent $MyInvocation.MyCommand.Definition
$driverDir    = Join-Path $scriptDir "driver"
if (-not (Test-Path (Join-Path $driverDir "vbhfvaio64_win7.inf"))) {
    $driverDir = $scriptDir
}
$infName      = "vbhfvaio64_win7.inf"
$sysName      = "vbaudio_hfvaio64_win7.sys"
$catName      = "vbaudio_hfvaio64_win7.cat"
$asioInstaller = Join-Path $scriptDir "HiFiCableAsioBridgeSetup.exe"

function Write-Log {
    param([string]$Msg, [string]$Level = "INFO")
    $ts   = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $line = "[$ts][$Level] $Msg"
    try { [Console]::Out.WriteLine($line) } catch {}
    try { [Console]::Out.Flush() } catch {}
    Add-Content -Path $LOG_FILE -Value $line -Encoding UTF8
}

trap {
    $errText = ($_ | Out-String).Trim()
    try {
        Write-Log "UNHANDLED EXCEPTION: $errText" "ERROR"
    } catch {}
    exit 1
}

function Invoke-PnpUtilLogged {
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [string]$Label = "pnputil"
    )

    try {
        $output = & pnputil.exe @Arguments 2>&1
        $exitCode = $LASTEXITCODE
        foreach ($line in @($output)) {
            if (-not [string]::IsNullOrWhiteSpace($line)) {
                Write-Log "$Label :: $line"
            }
        }
        Write-Log "$Label exit=$exitCode"
        return $exitCode
    } catch {
        Write-Log "$Label exception: $_" "WARN"
        return -1
    }
}

function Write-HiFiDiagnostics {
    param([string]$Reason = "snapshot")

    Write-Log "=== DIAGNOSTIC SNAPSHOT: $Reason ==="

    try {
        $pnp = Get-PnpDevice -PresentOnly:$false -ErrorAction SilentlyContinue | Where-Object {
            Test-IsHiFiPnpCandidate $_
        }
        foreach ($dev in @($pnp | Sort-Object -Property InstanceId -Unique)) {
            Write-Log ("  PNP: class={0} status={1} present={2} name={3} instance={4}" -f `
                [string]$dev.Class, [string]$dev.Status, [string]$dev.Present, [string]$dev.FriendlyName, [string]$dev.InstanceId)
        }
        if (-not $pnp) {
            Write-Log "  PNP: sin candidatos HiFi detectables" "WARN"
        }
    } catch {
        Write-Log "  PNP diagnostic error: $_" "WARN"
    }

    foreach ($type in @("Render", "Capture")) {
        try {
            $base = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$type"
            foreach ($dev in (Get-ChildItem $base -ErrorAction SilentlyContinue)) {
                $props = Get-ItemProperty "$($dev.PSPath)\Properties" -ErrorAction SilentlyContinue
                if (-not $props) { continue }
                $bestText = $null
                foreach ($prop in $props.PSObject.Properties) {
                    if ($prop.Name -like "PS*") { continue }
                    $decoded = Decode-PropVariantText $prop.Value
                    if (-not [string]::IsNullOrWhiteSpace($decoded)) {
                        $bestText = $decoded
                        break
                    }
                }
                Write-Log ("  MMDEV {0}: guid={1} text={2}" -f $type, $dev.PSChildName, $(if ($bestText) { $bestText } else { "<sin-texto>" }))
            }
        } catch {
            Write-Log "  MMDEV diagnostic error ($type): $_" "WARN"
        }
    }
}

function Test-AsioBridgeInstalled {
    $uninstallRoots = @(
        "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*",
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*"
    )

    foreach ($root in $uninstallRoots) {
        try {
            $match = Get-ItemProperty $root -ErrorAction SilentlyContinue |
                Where-Object {
                    $_.DisplayName -match "(?i)asio bridge|hi.?fi cable.+bridge|vb-audio.+hi.?fi"
                } |
                Select-Object -First 1
            if ($match) {
                return $true
            }
        } catch {}
    }

    $programFilesRoots = @(
        (Join-Path ${env:ProgramFiles(x86)} "VB"),
        (Join-Path ${env:ProgramFiles(x86)} "VB-Audio"),
        (Join-Path $env:ProgramFiles "VB"),
        (Join-Path $env:ProgramFiles "VB-Audio")
    )

    foreach ($root in $programFilesRoots | Where-Object { $_ -and (Test-Path $_) }) {
        try {
            $match = Get-ChildItem -Path $root -Recurse -File -ErrorAction SilentlyContinue |
                Where-Object {
                    $_.Name -match "(?i)asio.+bridge|hi.?fi.+bridge|hi.?fi.+control"
                } |
                Select-Object -First 1
            if ($match) {
                return $true
            }
        } catch {}
    }

    return $false
}

function Test-IsHiFiPnpCandidate {
    param([object]$Device)

    if ($null -eq $Device) { return $false }

    $fields = @(
        [string]$Device.FriendlyName,
        [string]$Device.Name,
        [string]$Device.InstanceId,
        [string]$Device.Class,
        [string]$Device.Present
    )

    try {
        if ($Device.HardwareID) {
            foreach ($hwid in @($Device.HardwareID)) {
                $fields += [string]$hwid
            }
        }
    } catch {}

    $joined = ($fields | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join " | "
    if ([string]::IsNullOrWhiteSpace($joined)) { return $false }

    return $joined -match '(?i)VBAudioHFVAIO|VB-Audio Hi-?Fi Cable|Hi-?Fi Cable|Echo PLUS|EchoAudio|VanySound'
}

function Get-HifiPreferenceScore {
    param(
        [string]$Name,
        [bool]$Present = $true,
        [string]$Type = "Any"
    )

    $score = 0
    $safeName = [string]$Name

    if ($Present) { $score += 100 }
    if ($safeName -match '(?i)^VanySound(\s*\(|$)') {
        $score += 80
    } elseif ($safeName -match '(?i)\bVanySound\b') {
        $score += 45
    } elseif ($safeName -match '(?i)^VB-Audio Hi-?Fi Cable(\s*\(|$)') {
        $score += 35
    } elseif ($safeName -match '(?i)^Hi-?Fi Cable Output(\s*\(|$)') {
        $score += 25
    }

    if ($Type -eq "Render" -and $safeName -match '(?i)^Altavoces\s+\(VanySound\)') {
        $score += 10
    }

    if ($safeName -match '(?i)(^|[\s(])\d+\s*-\s*') {
        $score -= 120
    }
    if ($safeName -match '(?i)\(\s*\d+\s*-\s*VanySound\s*\)') {
        $score -= 80
    }

    return $score
}

function Get-HiFiEndpointCandidates {
    param([bool]$PresentOnly = $true)

    $items = @()
    try {
        if ($PresentOnly) {
            $eps = @(Get-PnpDevice -Class AudioEndpoint -PresentOnly -ErrorAction SilentlyContinue | Where-Object {
                $_.InstanceId -match "MMDEVAPI" -and (Test-IsHiFiPnpCandidate $_)
            })
        } else {
            $eps = @(Get-PnpDevice -Class AudioEndpoint -PresentOnly:$false -ErrorAction SilentlyContinue | Where-Object {
                $_.InstanceId -match "MMDEVAPI" -and (Test-IsHiFiPnpCandidate $_)
            })
        }

        foreach ($ep in $eps) {
            $instanceId = [string]$ep.InstanceId
            $type = if ($instanceId -match '0\.0\.0\.') {
                "Render"
            } elseif ($instanceId -match '0\.0\.1\.') {
                "Capture"
            } else {
                "Unknown"
            }
            $friendlyName = [string]$ep.FriendlyName
            $present = ([string]$ep.Present -eq "True")
            $score = Get-HifiPreferenceScore -Name $friendlyName -Present:$present -Type $type

            $items += [pscustomobject]@{
                Type = $type
                FriendlyName = $friendlyName
                InstanceId = $instanceId
                Present = $present
                Score = $score
            }
        }
    } catch {}

    return @(
        $items | Sort-Object -Property `
            @{ Expression = "Type"; Descending = $false }, `
            @{ Expression = "Score"; Descending = $true }, `
            @{ Expression = "FriendlyName"; Descending = $false }, `
            @{ Expression = "InstanceId"; Descending = $false }
    )
}

function Get-HiFiRootCandidates {
    $items = @()
    try {
        $roots = @(Get-PnpDevice -Class Media -PresentOnly:$false -ErrorAction SilentlyContinue | Where-Object {
            ([string]$_.InstanceId -match '(?i)^ROOT\\MEDIA\\') -and (Test-IsHiFiPnpCandidate $_)
        })

        foreach ($root in $roots) {
            $friendlyName = [string]$root.FriendlyName
            $present = ([string]$root.Present -eq "True")
            $items += [pscustomobject]@{
                FriendlyName = $friendlyName
                InstanceId = [string]$root.InstanceId
                Present = $present
                Score = Get-HifiPreferenceScore -Name $friendlyName -Present:$present -Type "Root"
            }
        }
    } catch {}

    return @(
        $items | Sort-Object -Property `
            @{ Expression = "Score"; Descending = $true }, `
            @{ Expression = "FriendlyName"; Descending = $false }, `
            @{ Expression = "InstanceId"; Descending = $false }
    )
}

function Remove-PnpInstanceRobust {
    param(
        [Parameter(Mandatory = $true)][string]$InstanceId,
        [string]$Label = $InstanceId
    )

    $removed = $false
    try {
        $removeOutput = & pnputil.exe /remove-device "$InstanceId" 2>&1
        $removeCode = $LASTEXITCODE
        foreach ($line in @($removeOutput)) {
            if (-not [string]::IsNullOrWhiteSpace($line)) {
                Write-Log "    pnputil remove :: $line"
            }
        }
        if ($removeCode -eq 0) {
            Write-Log "    pnputil /remove-device OK: $Label"
            $removed = $true
        } else {
            Write-Log "    pnputil /remove-device exit=$removeCode para $Label" "WARN"
        }
    } catch {
        Write-Log "    pnputil /remove-device exception para ${Label}: $_" "WARN"
    }

    return $removed
}

function Cleanup-HiFiDuplicates {
    param([string]$Reason = "general")

    $changed = $false
    Write-Log "[Dedup] Revisando dispositivos HiFi duplicados ($Reason)..."

    $rootCandidates = @(Get-HiFiRootCandidates)
    if ($rootCandidates.Count -gt 1) {
        $keeper = $rootCandidates[0]
        Write-Log "  [Dedup] Conservando root principal: $($keeper.FriendlyName) | $($keeper.InstanceId)"
        foreach ($dup in @($rootCandidates | Select-Object -Skip 1)) {
            Write-Log "  [Dedup] Eliminando root duplicado: $($dup.FriendlyName) | $($dup.InstanceId)" "WARN"
            if (Remove-PnpInstanceRobust -InstanceId $dup.InstanceId -Label $dup.FriendlyName) {
                $changed = $true
            }
        }
    }

    $endpointCandidates = @(Get-HiFiEndpointCandidates -PresentOnly:$false)
    foreach ($type in @("Render", "Capture")) {
        $typed = @($endpointCandidates | Where-Object { $_.Type -eq $type })
        if ($typed.Count -gt 1) {
            $keeper = $typed[0]
            Write-Log "  [Dedup] Conservando $type principal: $($keeper.FriendlyName) | $($keeper.InstanceId)"
            foreach ($dup in @($typed | Select-Object -Skip 1)) {
                Write-Log "  [Dedup] Eliminando $type duplicado: $($dup.FriendlyName) | $($dup.InstanceId)" "WARN"
                if (Remove-PnpInstanceRobust -InstanceId $dup.InstanceId -Label "$type $($dup.FriendlyName)") {
                    $changed = $true
                }
            }
        }
    }

    if ($changed) {
        [void](Invoke-PnpUtilLogged -Arguments @("/scan-devices") -Label "PNPUTIL DEDUP SCAN")
        & net stop AudioEndpointBuilder /y 2>$null
        Start-Sleep -Seconds 1
        & net start AudioEndpointBuilder 2>$null
        & net start Audiosrv 2>$null
        Start-Sleep -Seconds 4
    }

    return $changed
}

function Enable-PnpInstanceRobust {
    param(
        [Parameter(Mandatory = $true)][string]$InstanceId,
        [string]$Label = $InstanceId
    )

    $success = $false

    try {
        Enable-PnpDevice -InstanceId $InstanceId -Confirm:$false -ErrorAction Stop | Out-Null
        Write-Log "    Enable-PnpDevice OK: $Label"
        $success = $true
    } catch {
        Write-Log "    Enable-PnpDevice fallo para ${Label}: $_" "WARN"
    }

    if (-not $success) {
        try {
            $enableOutput = & pnputil.exe /enable-device "$InstanceId" 2>&1
            $pnputilCode = $LASTEXITCODE
            foreach ($line in @($enableOutput)) {
                if (-not [string]::IsNullOrWhiteSpace($line)) {
                    Write-Log "    pnputil enable :: $line"
                }
            }
            if ($pnputilCode -eq 0) {
                Write-Log "    pnputil /enable-device OK: $Label"
                $success = $true
            } else {
                Write-Log "    pnputil /enable-device exit=$pnputilCode para $Label" "WARN"
            }
        } catch {
            Write-Log "    pnputil /enable-device exception para ${Label}: $_" "WARN"
        }
    }

    try {
        $restartOutput = & pnputil.exe /restart-device "$InstanceId" 2>&1
        $restartCode = $LASTEXITCODE
        foreach ($line in @($restartOutput)) {
            if (-not [string]::IsNullOrWhiteSpace($line)) {
                Write-Log "    pnputil restart :: $line"
            }
        }
        if ($restartCode -eq 0) {
            Write-Log "    pnputil /restart-device OK: $Label"
            $success = $true
        }
    } catch {
        Write-Log "    pnputil /restart-device exception para ${Label}: $_" "WARN"
    }

    return $success
}

function Set-MMDeviceStateActive {
    param(
        [Parameter(Mandatory = $true)][ValidateSet("Render", "Capture")][string]$Type,
        [Parameter(Mandatory = $true)][string]$Guid
    )

    $deviceKeyPath = "SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Type\$Guid"
    $regPath = "HKEY_LOCAL_MACHINE\$deviceKeyPath\"

    try {
        if (Test-Path $echoTools) {
            & $echoTools $regPath 2>$null | Out-Null
            Start-Sleep -Milliseconds 250
        }
    } catch {}

    try {
        $deviceKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($deviceKeyPath, $true)
        if (-not $deviceKey) {
            Write-Log "  [MMDev Final] $Type $Guid -> no se pudo abrir la clave del dispositivo." "WARN"
            return $false
        }

        $deviceKey.SetValue("DeviceState", 1, [Microsoft.Win32.RegistryValueKind]::DWord)
        $deviceKey.Dispose()
        Write-Log "  [MMDev Final] $Type $Guid -> DeviceState=1"
        return $true
    } catch {
        Write-Log "  [MMDev Final] $Type $Guid error activando DeviceState: $_" "WARN"
        return $false
    }
}

function Invoke-HiddenScheduledProcess {
    param(
        [Parameter(Mandatory = $true)][string]$AttemptName,
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$ArgumentList = @(),
        [switch]$UseCmdWrapper,
        [int]$TimeoutSeconds = 20
    )

    $taskName = "VanySound_ASIO_" + ([Guid]::NewGuid().ToString("N"))
    $wrapperPath = Join-Path $env:TEMP ($taskName + ".ps1")
    $exitPath = Join-Path $env:TEMP ($taskName + ".exit.txt")
    $stdoutPath = Join-Path $env:TEMP ($taskName + ".stdout.txt")
    $stderrPath = Join-Path $env:TEMP ($taskName + ".stderr.txt")

    Remove-Item $wrapperPath, $exitPath, $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue

    $wrappedArgsLiteral = ($ArgumentList | ForEach-Object { "'" + ($_.Replace("'", "''")) + "'" }) -join ", "
    $filePathLiteral = "'" + ($FilePath.Replace("'", "''")) + "'"
    $stdoutLiteral = "'" + ($stdoutPath.Replace("'", "''")) + "'"
    $stderrLiteral = "'" + ($stderrPath.Replace("'", "''")) + "'"
    $exitLiteral = "'" + ($exitPath.Replace("'", "''")) + "'"
    $scriptDirLiteral = "'" + ($scriptDir.Replace("'", "''")) + "'"

    $wrapperContent = @"
`$ErrorActionPreference = 'Continue'
`$filePath = $filePathLiteral
`$argumentList = @($wrappedArgsLiteral)
`$stdoutPath = $stdoutLiteral
`$stderrPath = $stderrLiteral
`$exitPath = $exitLiteral
`$workingDirectory = $scriptDirLiteral
`$useCmdWrapper = $($(if ($UseCmdWrapper) { '$true' } else { '$false' }))

try {
    if (`$useCmdWrapper) {
        `$rawCommand = '"' + `$filePath + '"'
        if (`$argumentList.Count -gt 0) {
            `$rawCommand += ' ' + (`$argumentList -join ' ')
        }
        `$psi = New-Object System.Diagnostics.ProcessStartInfo
        `$psi.FileName = 'cmd.exe'
        `$psi.Arguments = '/c ' + `$rawCommand
    } else {
        `$psi = New-Object System.Diagnostics.ProcessStartInfo
        `$psi.FileName = `$filePath
        if (`$argumentList.Count -gt 0) {
            `$psi.Arguments = [string]::Join(' ', `$argumentList)
        }
    }

    `$psi.WorkingDirectory = `$workingDirectory
    `$psi.UseShellExecute = `$false
    `$psi.CreateNoWindow = `$true
    `$psi.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    `$psi.RedirectStandardOutput = `$true
    `$psi.RedirectStandardError = `$true

    `$proc = New-Object System.Diagnostics.Process
    `$proc.StartInfo = `$psi
    [void]`$proc.Start()
    `$proc.WaitForExit()
    [System.IO.File]::WriteAllText(`$stdoutPath, `$proc.StandardOutput.ReadToEnd())
    [System.IO.File]::WriteAllText(`$stderrPath, `$proc.StandardError.ReadToEnd())
    [System.IO.File]::WriteAllText(`$exitPath, [string]`$proc.ExitCode)
} catch {
    [System.IO.File]::WriteAllText(`$stderrPath, (`$_ | Out-String))
    [System.IO.File]::WriteAllText(`$exitPath, '-999')
}
"@

    Set-Content -Path $wrapperPath -Value $wrapperContent -Encoding UTF8

    $runTime = (Get-Date).AddMinutes(1)
    $sd = $runTime.ToString("MM/dd/yyyy")
    $st = $runTime.ToString("HH:mm")
    $taskCommand = "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$wrapperPath`""

    try {
        $createOutput = & schtasks.exe /Create /TN $taskName /TR $taskCommand /SC ONCE /SD $sd /ST $st /RU SYSTEM /RL HIGHEST /F 2>&1
        foreach ($line in @($createOutput)) {
            if (-not [string]::IsNullOrWhiteSpace($line)) {
                Write-Log "ASIO task create:: $line"
            }
        }

        $runOutput = & schtasks.exe /Run /TN $taskName 2>&1
        foreach ($line in @($runOutput)) {
            if (-not [string]::IsNullOrWhiteSpace($line)) {
                Write-Log "ASIO task run:: $line"
            }
        }

        $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
        while (-not (Test-Path $exitPath) -and (Get-Date) -lt $deadline) {
            Start-Sleep -Milliseconds 500
        }

        if (-not (Test-Path $exitPath)) {
            Write-Log "ASIO attempt [$AttemptName] timeout tras ${TimeoutSeconds}s dentro de tarea oculta; cancelando." "WARN"
            try { & schtasks.exe /End /TN $taskName 2>$null | Out-Null } catch {}
            return -998
        }

        foreach ($line in @(Get-Content -Path $stdoutPath -ErrorAction SilentlyContinue)) {
            if (-not [string]::IsNullOrWhiteSpace($line)) {
                Write-Log "ASIO hidden stdout:: $line"
            }
        }
        foreach ($line in @(Get-Content -Path $stderrPath -ErrorAction SilentlyContinue)) {
            if (-not [string]::IsNullOrWhiteSpace($line)) {
                Write-Log "ASIO hidden stderr:: $line" "WARN"
            }
        }

        $exitCode = 0
        try {
            $exitCode = [int](Get-Content -Path $exitPath -ErrorAction Stop | Select-Object -First 1)
        } catch {
            $exitCode = -999
        }
        return $exitCode
    } finally {
        try { & schtasks.exe /Delete /TN $taskName /F 2>$null | Out-Null } catch {}
        Remove-Item $wrapperPath, $exitPath, $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-AsioInstallerAttempt {
    param(
        [Parameter(Mandatory = $true)][string]$AttemptName,
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$ArgumentList = @(),
        [switch]$UseCmdWrapper,
        [int]$TimeoutSeconds = 20
    )

    try {
        $renderedArgs = if ($ArgumentList.Count -gt 0) { $ArgumentList -join ' ' } else { '(sin argumentos)' }
        if ($UseCmdWrapper) {
            Write-Log "ASIO attempt [$AttemptName] hidden-task cmd.exe /c `"$FilePath`" $renderedArgs"
        } else {
            Write-Log "ASIO attempt [$AttemptName] hidden-task $FilePath $renderedArgs"
        }

        $exitCode = Invoke-HiddenScheduledProcess `
            -AttemptName $AttemptName `
            -FilePath $FilePath `
            -ArgumentList $ArgumentList `
            -UseCmdWrapper:$UseCmdWrapper `
            -TimeoutSeconds $TimeoutSeconds

        if ($exitCode -eq -998) {
            $installerName = [System.IO.Path]::GetFileNameWithoutExtension($FilePath)
            if (-not [string]::IsNullOrWhiteSpace($installerName)) {
                Get-Process -Name $installerName -ErrorAction SilentlyContinue | ForEach-Object {
                    try {
                        Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
                        Write-Log "ASIO attempt [$AttemptName] proceso residual cerrado: $($_.ProcessName) pid=$($_.Id)" "WARN"
                    } catch {}
                }
            }
        }

        Write-Log "ASIO attempt [$AttemptName] exit=$exitCode"
        return $exitCode
    } catch {
        Write-Log "ASIO attempt [$AttemptName] exception: $_" "WARN"
        return -999
    }
}

function Install-AsioBridgeIfNeeded {
    if (-not (Test-Path $asioInstaller)) {
        Write-Log "ERROR: HiFiCableAsioBridgeSetup.exe no encontrado en $scriptDir" "ERROR"
        return $false
    }

    if (Test-AsioBridgeInstalled) {
        Write-Log "ASIO Bridge ya detectado. Saltando instalacion silenciosa."
        return $true
    }

    Write-Log "Instalando ASIO Bridge silenciosamente desde $asioInstaller ..."
    $attempts = @(
        @{
            Name = "legacy-cmd"
            Args = @('40F3F4:"-h -i -H -n"')
            Cmd = $true
        },
        @{
            Name = "legacy-direct"
            Args = @('40F3F4:"-h -i -H -n"')
            Cmd = $false
        },
        @{
            Name = "standard-s"
            Args = @('/S')
            Cmd = $false
        }
    )

    foreach ($attempt in $attempts) {
        $exitCode = Invoke-AsioInstallerAttempt `
            -AttemptName $attempt.Name `
            -FilePath $asioInstaller `
            -ArgumentList $attempt.Args `
            -UseCmdWrapper:$attempt.Cmd `
            -TimeoutSeconds 20

        Start-Sleep -Seconds 2
        if (Test-AsioBridgeInstalled) {
            Write-Log "ASIO Bridge instalado y detectado tras intento '$($attempt.Name)'."
            return $true
        }

        if ($exitCode -eq 0) {
            Write-Log "WARN: intento '$($attempt.Name)' devolvio exit 0 pero la heuristica aun no detecta ASIO Bridge." "WARN"
        } elseif ($exitCode -eq -998) {
            Write-Log "WARN: intento '$($attempt.Name)' agotado por timeout; continuando con el siguiente metodo." "WARN"
        }
    }

    Write-Log "ERROR: todos los intentos silenciosos de ASIO Bridge fallaron o no dejaron huella detectable." "ERROR"
    return $false
}

# ======================================================
# PASO 1 . Auto-elevación
# ======================================================
$esAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $esAdmin -and $SkipSelfElevation) {
    Write-Log "ERROR: SkipSelfElevation fue solicitado pero el proceso no esta elevado." "ERROR"
    exit 1
}
if (-not $esAdmin) {
    Write-Host "[*] Relanzando como Administrador..." -ForegroundColor Yellow
    $elevatedArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$($MyInvocation.MyCommand.Definition)`"")
    if ($ConsoleLog) {
        $elevatedArgs += "-ConsoleLog"
    }
    Start-Process powershell.exe `
        -ArgumentList ($elevatedArgs -join " ") `
        -Verb RunAs -Wait
    exit
}

Write-Log "=== INICIO INSTALACION HiFi Cable (SetupAPI) ==="
Write-Log "OS: $([System.Environment]::OSVersion.VersionString) | Arch: $env:PROCESSOR_ARCHITECTURE"

# ======================================================
# PASO 2 . Verificar archivos del driver
# ======================================================
foreach ($f in @($infName, $sysName, $catName)) {
    $fp = Join-Path $driverDir $f
    if (-not (Test-Path $fp)) {
        Write-Log "ERROR: Falta archivo '$f' en $driverDir" "ERROR"
        Write-Host "`n[X] Falta: $fp`n    Asegurate de incluir la carpeta 'driver\' junto al script." -ForegroundColor Red
        exit 1
    }
}
$infPath = Join-Path $driverDir $infName
$catPath = Join-Path $driverDir $catName
$sysPath = Join-Path $driverDir $sysName
Write-Log "Archivos del driver encontrados en: $driverDir"

# ======================================================
# PASO 3 . ¿Ya instalado?
# ======================================================
function Test-HifiInstalado {
    # PnP Device (más fiable que WMI)
    try {
        $pnp = Get-PnpDevice -ErrorAction SilentlyContinue |
               Where-Object { $_.HardwareID -match "VBAudioHFVAIO" -or $_.FriendlyName -match "(?i)hi.fi cable" }
        if ($pnp) { return $true }
    } catch {}

    # WMI fallback
    $d = Get-WmiObject Win32_SoundDevice -ErrorAction SilentlyContinue |
         Where-Object { $_.Name -match "(?i)hifi|hi.fi cable" }
    if ($d) { return $true }

    return $false
}

function Test-HifiEndpointsPresent {
    $candidates = @(Get-HiFiEndpointCandidates -PresentOnly:$true)
    $render = @($candidates | Where-Object { $_.Type -eq "Render" })
    $capture = @($candidates | Where-Object { $_.Type -eq "Capture" })

    return [pscustomobject]@{
        RenderCount = $render.Count
        CaptureCount = $capture.Count
    }
}

$skipDriverInstall = $false
$repairExistingDriver = $false
if (Test-HifiInstalado) {
    [void](Cleanup-HiFiDuplicates -Reason "always-repair")
    Write-Log "HiFi Cable detectado. Forzando reparacion/reinstalacion completa del driver..."
    Write-Host "`n[*] Reparando HiFi Cable (siempre se reinstala)..." -ForegroundColor Yellow
    $repairExistingDriver = $true
}

if (-not $skipDriverInstall) {

# ======================================================
# PASO 4 . Copiar .sys al destino (Windows\System32\drivers)
#          SetupAPI lo hara, pero si falla lo hacemos manual
# ======================================================
$sysTarget = Join-Path $env:SystemRoot "System32\drivers\$sysName"
if (-not (Test-Path $sysTarget)) {
    try {
        Copy-Item $sysPath $sysTarget -Force
        Write-Log "Driver .sys copiado a: $sysTarget"
    } catch {
        Write-Log "Advertencia copiando .sys: $_ (SetupAPI lo gestionara)" "WARN"
    }
}

# ======================================================
# PASO 5 . Importar certificado .cat a TrustedPublisher y Root
# ======================================================
Write-Log "Importando certificado de firma digital..."
try {
    # Metodo primario: Get-AuthenticodeSignature del .sys (no requiere System.Security.Pkcs)
    $cert = (Get-AuthenticodeSignature -FilePath $sysPath -ErrorAction SilentlyContinue).SignerCertificate

    if (-not $cert) {
        # Fallback: desde el .inf
        $cert = (Get-AuthenticodeSignature -FilePath $infPath -ErrorAction SilentlyContinue).SignerCertificate
    }

    if (-not $cert) {
        # Fallback: descodificar el .cat via System.Security.Pkcs
        try { Add-Type -AssemblyName System.Security -ErrorAction Stop } catch {}
        try {
            $cms = New-Object System.Security.Cryptography.Pkcs.SignedCms
            $cms.Decode([System.IO.File]::ReadAllBytes($catPath))
            $cert = $cms.SignerInfos[0].Certificate
        } catch {}
    }

    if ($cert) {
        Write-Log "  Certificado: $($cert.Subject)"

        foreach ($storeName in @("TrustedPublisher")) {
            $st = New-Object System.Security.Cryptography.X509Certificates.X509Store(
                $storeName,
                [System.Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine)
            $st.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
            $st.Add($cert)
            $st.Close()
        }

        # Root CA
        $chain = New-Object System.Security.Cryptography.X509Certificates.X509Chain
        [void]$chain.Build($cert)
        foreach ($el in $chain.ChainElements) {
            if ($el.Certificate.Subject -eq $el.Certificate.Issuer) {
                $rootSt = New-Object System.Security.Cryptography.X509Certificates.X509Store(
                    "Root",
                    [System.Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine)
                $rootSt.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
                $rootSt.Add($el.Certificate)
                $rootSt.Close()
                Write-Log "  Root CA importado: $($el.Certificate.Subject)"
                break
            }
        }
        Write-Log "  Certificado importado OK."
    } else {
        Write-Log "  Sin certificado en .cat (no critico)" "WARN"
    }
} catch {
    Write-Log "  Error importando cert: $_ (puede continuar)" "WARN"
}

# ======================================================
# PASO 6 . Policy de driver signing -> silenciosa
# ======================================================
$regDS = "HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\Driver Signing"
$origVal = $null
try {
    if (-not (Test-Path $regDS)) { New-Item $regDS -Force | Out-Null }
    $origVal = (Get-ItemProperty $regDS "BehaviorOnFailedVerify" -ErrorAction SilentlyContinue)."BehaviorOnFailedVerify"
    Set-ItemProperty $regDS "BehaviorOnFailedVerify" -Value 0 -Type DWord -Force
    Write-Log "Driver signing policy -> 0 (silencioso)"
} catch { Write-Log "Policy: $_ (no critico)" "WARN" }

# ======================================================
# PASO 7 . SetupAPI: Crear dispositivo virtual + instalar driver
#          Equivalente a:  devcon.exe install driver.inf VBAudioHFVAIO
# ======================================================
Write-Log "Compilando shim SetupAPI..."

[void](Invoke-PnpUtilLogged -Arguments @("/add-driver", $infPath, "/install") -Label "PNPUTIL PRESTAGE")

Add-Type @"
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class SetupAPIShim {

    [StructLayout(LayoutKind.Sequential)]
    public struct SP_DEVINFO_DATA {
        public uint  cbSize;
        public Guid  ClassGuid;
        public uint  DevInst;
        public IntPtr Reserved;
    }

    // -- setupapi.dll ------------------------------
    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr SetupDiCreateDeviceInfoList(ref Guid ClassGuid, IntPtr hwndParent);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool SetupDiCreateDeviceInfo(
        IntPtr DeviceInfoSet, string DeviceName, ref Guid ClassGuid,
        string DeviceDescription, IntPtr hwndParent, uint CreationFlags,
        ref SP_DEVINFO_DATA DeviceInfoData);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool SetupDiSetDeviceRegistryProperty(
        IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData,
        uint Property, byte[] PropertyBuffer, uint PropertyBufferSize);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool SetupDiCallClassInstaller(
        uint  InstallFunction, IntPtr DeviceInfoSet,
        ref SP_DEVINFO_DATA DeviceInfoData);

    [DllImport("setupapi.dll", SetLastError = true)]
    public static extern bool SetupDiDestroyDeviceInfoList(IntPtr DeviceInfoSet);

    // -- newdev.dll --------------------------------
    [DllImport("newdev.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool UpdateDriverForPlugAndPlayDevices(
        IntPtr hwndParent, string HardwareId, string FullInfPath,
        uint InstallFlags, ref bool bRebootRequired);

    // -- Constantes --------------------------------
    const uint DICD_GENERATE_ID    = 0x00000001;
    const uint SPDRP_HARDWAREID    = 0x00000001;
    const uint DIF_REGISTERDEVICE  = 0x00000019;
    const uint INSTALLFLAG_FORCE   = 0x00000001;
    static readonly IntPtr INVALID_HANDLE = new IntPtr(-1);

    /// <summary>
    /// Crea un nodo de dispositivo raíz (Root\MEDIA\XXXX) con el HardwareID
    /// especificado e instala el driver desde el INF dado.
    /// Equivale a: devcon.exe install <inf> <hwid>
    /// </summary>
    public static bool InstallVirtualDevice(string hwid, string infFullPath, out bool rebootRequired) {
        rebootRequired = false;
        Guid mediaGuid = new Guid("{4d36e96c-e325-11ce-bfc1-08002be10318}");

        IntPtr devSet = SetupDiCreateDeviceInfoList(ref mediaGuid, IntPtr.Zero);
        if (devSet == INVALID_HANDLE)
            throw new Win32Exception(Marshal.GetLastWin32Error(), "SetupDiCreateDeviceInfoList");

        try {
            var devData = new SP_DEVINFO_DATA { cbSize = (uint)Marshal.SizeOf<SP_DEVINFO_DATA>() };

            // Crear device info con ID generado automáticamente
            if (!SetupDiCreateDeviceInfo(devSet, "MEDIA", ref mediaGuid,
                    null, IntPtr.Zero, DICD_GENERATE_ID, ref devData))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "SetupDiCreateDeviceInfo");

            // Asignar Hardware ID (multi-string Unicode: hwid\0\0)
            byte[] hwBytes = Encoding.Unicode.GetBytes(hwid + "\0\0");
            if (!SetupDiSetDeviceRegistryProperty(devSet, ref devData,
                    SPDRP_HARDWAREID, hwBytes, (uint)hwBytes.Length))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "SPDRP_HARDWAREID");

            // Registrar el dispositivo en el árbol PnP
            if (!SetupDiCallClassInstaller(DIF_REGISTERDEVICE, devSet, ref devData))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "DIF_REGISTERDEVICE");

            // Instalar el driver (con force flag, luego sin él como fallback)
            bool rb = false;
            bool ok = UpdateDriverForPlugAndPlayDevices(
                IntPtr.Zero, hwid, infFullPath, INSTALLFLAG_FORCE, ref rb);
            if (!ok) {
                int err = Marshal.GetLastWin32Error();
                if (err == 259 /* NO_MORE_ITEMS - no matching device yet */) {
                    // Normal si el device acaba de crearse; intentamos sin FORCE
                    ok = UpdateDriverForPlugAndPlayDevices(
                        IntPtr.Zero, hwid, infFullPath, 0, ref rb);
                }
            }
            rebootRequired = rb;
            return ok;
        }
        finally {
            SetupDiDestroyDeviceInfoList(devSet);
        }
    }

    /// <summary>
    /// Solo actualiza el driver (para cuando el dispositivo ya existe).
    /// </summary>
    public static bool UpdateDriver(string hwid, string infFullPath, out bool rebootRequired) {
        bool rb = false;
        bool ok = UpdateDriverForPlugAndPlayDevices(
            IntPtr.Zero, hwid, infFullPath, INSTALLFLAG_FORCE, ref rb);
        if (!ok) UpdateDriverForPlugAndPlayDevices(IntPtr.Zero, hwid, infFullPath, 0, ref rb);
        rebootRequired = rb;
        return ok;
    }
}
"@ -ErrorAction Stop

Write-Log "SetupAPI shim compilado."

# ======================================================
# PASO 8 . Ejecutar la instalación del driver
# ======================================================
if ($repairExistingDriver) {
    Write-Log "Reparando driver HiFi existente sin crear un nuevo dispositivo virtual..."
    Write-Host "`n[*] Reparando HiFi Cable existente..." -ForegroundColor Cyan
} else {
    Write-Log "Creando dispositivo virtual Root\\MEDIA\\$HARDWARE_ID e instalando driver..."
    Write-Host "`n[*] Instalando HiFi Cable (SetupAPI silencioso)..." -ForegroundColor Cyan
}

$reboot  = $false
$success = $false
$errMsg  = ""

try {
    if ($repairExistingDriver) {
        Write-Log "HiFi base ya existe; usando UpdateDriver() para reparar el dispositivo existente sin crear otro root device..."
        $success = [SetupAPIShim]::UpdateDriver($HARDWARE_ID, $infPath, [ref]$reboot)
        if ($success) {
            Write-Log "UpdateDriver() exitoso. Reboot necesario: $reboot"
        } else {
            $win32Err = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
            $errMsg = "Win32 error: $win32Err"
            Write-Log "UpdateDriver() retorno false. $errMsg" "WARN"
        }
    } else {
        $success = [SetupAPIShim]::InstallVirtualDevice($HARDWARE_ID, $infPath, [ref]$reboot)
        if ($success) {
            Write-Log "InstallVirtualDevice() exitoso. Reboot necesario: $reboot"
        } else {
            $win32Err = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
            $errMsg = "Win32 error: $win32Err"
            Write-Log "InstallVirtualDevice() retorno false. $errMsg" "WARN"
        }
    }
} catch {
    $errMsg = $_.Exception.Message
    Write-Log "Excepcion en instalacion/reparacion SetupAPI: $errMsg" "WARN"

    # Fallback: UpdateDriver (para si el device ya existe pero sin driver)
    try {
        Write-Log "Intentando UpdateDriver() como fallback..."
        $success = [SetupAPIShim]::UpdateDriver($HARDWARE_ID, $infPath, [ref]$reboot)
        Write-Log "UpdateDriver() resultado: $success | Reboot: $reboot"
    } catch {
        Write-Log "UpdateDriver() tambien fallo: $($_.Exception.Message)" "WARN"
    }
}

[void](Invoke-PnpUtilLogged -Arguments @("/scan-devices") -Label "PNPUTIL SCAN")

# ======================================================
# PASO 9 . Restaurar política
# ======================================================
try {
    if ($null -ne $origVal) {
        Set-ItemProperty $regDS "BehaviorOnFailedVerify" -Value $origVal -Type DWord -Force
    } else {
        Remove-ItemProperty $regDS "BehaviorOnFailedVerify" -ErrorAction SilentlyContinue
    }
    Write-Log "Driver signing policy restaurada."
} catch {}

# Limpiar .sys copiado si SetupAPI lo movió al DriverStore
# (SetupAPI gestiona esto automáticamente, no necesitamos borrarlo)

# ======================================================
# PASO 10 . Resultado
# ======================================================
Start-Sleep -Seconds 2

$instaladoOk = Test-HifiInstalado

if ($instaladoOk -or $success) {
    Write-Log "EXITO: HiFi Cable instalado (detectado=$instaladoOk, api_ok=$success, reboot=$reboot)"
    Write-Host "`n[OK] HiFi Cable instalado correctamente!" -ForegroundColor Green
    if ($reboot) {
        Write-Host "[!] Se requiere REINICIAR Windows para activar el dispositivo." -ForegroundColor Yellow
        # Registrar tarea programada que aplica VanySound + 48kHz despues del reboot
        $applyScript = Join-Path $scriptDir "apply_echoplus_now.ps1"
        if (Test-Path $applyScript) {
            $action  = New-ScheduledTaskAction -Execute "powershell.exe" `
                -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$applyScript`""
            $trigger = New-ScheduledTaskTrigger -AtLogOn
            $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 5) -DeleteExpiredTaskAfter (New-TimeSpan -Seconds 1)
            Register-ScheduledTask -TaskName "VanySound_ApplyAfterReboot" `
                -Action $action -Trigger $trigger -RunLevel Highest `
                -Settings $settings -Force -ErrorAction SilentlyContinue | Out-Null
            Write-Log "Tarea programada 'VanySound_ApplyAfterReboot' registrada para post-reboot."
            Write-Host "[i] Tarea registrada: VanySound se configurara automaticamente al reiniciar." -ForegroundColor Cyan
        }
    } else {
        Write-Host "[i] El dispositivo deberia aparecer sin reiniciar." -ForegroundColor Cyan
    }
    $finalCode = 0
} else {
    Write-Log "RESULTADO INCIERTO: instalado=$instaladoOk, api_ok=$success. Error: $errMsg" "WARN"
    Write-Host "`n[!] Instalacion completada pero no confirmada. REINICIA Windows." -ForegroundColor Yellow
    Write-Host "    Si no aparece despues del reboot, contacta soporte." -ForegroundColor Gray
    $finalCode = 0  # No es un error fatal: el driver puede aparecer tras reboot
}

Write-Log "=== FIN driver | instalado=$instaladoOk | api_ok=$success | reboot=$reboot ==="

} # fin if (-not $skipDriverInstall)

$asioInstallOk = Install-AsioBridgeIfNeeded
if (-not $asioInstallOk) {
    Write-Log "WARN: ASIO Bridge no pudo instalarse silenciosamente. Se continua con la suite porque el runtime principal no depende de este paso." "WARN"
    Write-Host "`n[!] ASIO Bridge no pudo instalarse silenciosamente. La suite continuara y el instalador quedara incluido para revision manual si hiciera falta." -ForegroundColor Yellow
}

# ==============================================================
# PASO 11 . Renombrar a "VanySound" + 48kHz/24-bit (Playback + Recording)
# TRIPLE ESTRATEGIA: PnP -> PROPVARIANT decode -> scan completo
# Sin dependencia de idioma de Windows (EN/ES)
# ==============================================================
Write-Log "PASO 11: Configurando 'VanySound' + 48kHz/24bit..."

# EchoTools.exe: toma ownership recursivo de las claves del registro
# (replicacion de ExtraTools.exe de RaraAudioApp)
$echoTools = Join-Path $scriptDir "EchoTools.exe"
if (-not (Test-Path $echoTools)) {
    Write-Log "WARN: EchoTools.exe no encontrado en $scriptDir" "WARN"
}

# WAVEFORMATEXTENSIBLE 48000Hz PCM 24-bit.
# Render: 7.1 surround (8 canales, mascara 0x63F)
# Capture: stereo (2 canales, mascara 0x3)
$FMT_48K_24BIT_7_1 = "41,00,00,00,01,00,00,00,fe,ff,08,00,80,bb,00,00,00,94,11,00,18,00,18,00,16,00,18,00,3f,06,00,00,01,00,00,00,00,00,10,00,80,00,00,aa,00,38,9b,71"
$FMT_48K_24BIT_STEREO = "41,00,00,00,01,00,00,00,fe,ff,02,00,80,bb,00,00,00,65,04,00,06,00,18,00,16,00,18,00,03,00,00,00,01,00,00,00,00,00,10,00,80,00,00,aa,00,38,9b,71"
$SPEAKER_MASK_7_1 = 0x0000063F
$PKEY_PHYSICAL_SPEAKERS = "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},3"
$PKEY_FULLRANGE_SPEAKERS = "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},6"

$setupDir = "C:\Program Files\win\Setup"
if (-not (Test-Path $setupDir)) { New-Item $setupDir -ItemType Directory -Force | Out-Null }

    # Helper: Clase C# para modificar SPDRP_FRIENDLYNAME via SetupAPI
    $pnpCs = @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public class PnPRenamer {
    [DllImport("setupapi.dll", CharSet = CharSet.Auto)]
    static extern IntPtr SetupDiGetClassDevs(ref Guid ClassGuid, string Enumerator, IntPtr hwndParent, uint Flags);

    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiEnumDeviceInfo(IntPtr DeviceInfoSet, uint MemberIndex, ref SP_DEVINFO_DATA DeviceInfoData);

    [DllImport("setupapi.dll", CharSet = CharSet.Auto, SetLastError = true)]
    static extern bool SetupDiGetDeviceRegistryProperty(IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData, uint Property, out uint PropertyRegDataType, byte[] PropertyBuffer, uint PropertyBufferSize, out uint RequiredSize);

    [DllImport("setupapi.dll", CharSet = CharSet.Auto, SetLastError = true)]
    static extern bool SetupDiSetDeviceRegistryProperty(IntPtr DeviceInfoSet, ref SP_DEVINFO_DATA DeviceInfoData, uint Property, byte[] PropertyBuffer, uint PropertyBufferSize);

    [DllImport("setupapi.dll")]
    static extern bool SetupDiDestroyDeviceInfoList(IntPtr DeviceInfoSet);

    [StructLayout(LayoutKind.Sequential)]
    struct SP_DEVINFO_DATA {
        public uint cbSize;
        public Guid ClassGuid;
        public uint DevInst;
        public IntPtr Reserved;
    }

    const uint DIGCF_PRESENT = 2;
    const uint SPDRP_FRIENDLYNAME = 0x0000000C;
    const uint SPDRP_DEVICEDESC = 0x00000000;

    public static void RenameMediaDevices(string targetSubstring, string newName) {
        Guid mediaGuid = new Guid("4d36e96c-e325-11ce-bfc1-08002be10318"); // MEDIA class
        IntPtr hDevInfo = SetupDiGetClassDevs(ref mediaGuid, null, IntPtr.Zero, DIGCF_PRESENT);
        
        if (hDevInfo == new IntPtr(-1)) return;

        SP_DEVINFO_DATA devData = new SP_DEVINFO_DATA();
        devData.cbSize = (uint)Marshal.SizeOf(devData);

        for (uint i = 0; SetupDiEnumDeviceInfo(hDevInfo, i, ref devData); i++) {
            uint propType;
            uint reqSize;
            byte[] buf = new byte[1024];
            
            string desc = "";
            if (SetupDiGetDeviceRegistryProperty(hDevInfo, ref devData, SPDRP_DEVICEDESC, out propType, buf, 1024, out reqSize)) {
                desc = Encoding.Unicode.GetString(buf, 0, (int)reqSize).TrimEnd('\0');
            }
            
            if (desc.IndexOf(targetSubstring, StringComparison.OrdinalIgnoreCase) >= 0) {
                byte[] newNameBuf = Encoding.Unicode.GetBytes(newName + "\0");
                SetupDiSetDeviceRegistryProperty(hDevInfo, ref devData, SPDRP_FRIENDLYNAME, newNameBuf, (uint)newNameBuf.Length);
            }
        }
        SetupDiDestroyDeviceInfoList(hDevInfo);
    }
}
"@
    try { Add-Type -TypeDefinition $pnpCs } catch {}

    # --- Método 1: Get-PnpDevice (idioma-agnóstico, nombre desde .inf driver) ---
function Normalize-GuidList {
    param([object]$Value)

    if ($null -eq $Value) { return @() }
    return ,@($Value | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })
}

    function Decode-HiFiPropText {
        param([object]$Raw)

        if ($null -eq $Raw) { return $null }
        if ($Raw -is [string]) { return $Raw }
        if ($Raw -is [byte[]] -and $Raw.Length -ge 10) {
            try {
                $vt = [BitConverter]::ToUInt16($Raw, 0)
                if ($vt -eq 31) {
                    return ([System.Text.Encoding]::Unicode.GetString($Raw, 8, $Raw.Length - 8)).TrimEnd([char]0).Trim()
                }
            } catch {}
            try {
                return ([System.Text.Encoding]::Unicode.GetString($Raw)).TrimEnd([char]0).Trim()
            } catch {}
        }

        return $null
    }

function Remove-LegacyBrokenEndpointLabel {
    param(
        [Parameter(Mandatory = $true)][ValidateSet("Render", "Capture")][string]$Type,
        [Parameter(Mandatory = $true)][string]$Guid
    )

    $legacyProp = "{b3f8fa53-0004-438e-9003-51a46e139bfc},2"
    $propsPath = "SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Type\$Guid\Properties"

    try {
        $propsKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($propsPath, $true)
        if (-not $propsKey) { return $false }

        $legacyRaw = $null
        try { $legacyRaw = $propsKey.GetValue($legacyProp, $null) } catch {}
        if ($null -eq $legacyRaw) {
            $propsKey.Dispose()
            return $false
        }

        $legacyText = Decode-HiFiPropText $legacyRaw
        $descText = Decode-HiFiPropText ($propsKey.GetValue("{a45c254e-df1c-4efd-8020-67d146a850e0},2", $null))
        $ifaceText = Decode-HiFiPropText ($propsKey.GetValue("{b3f8fa53-0004-438e-9003-51a46e139bfc},6", $null))
        $joined = @($legacyText, $descText, $ifaceText) -join " | "

        if ($joined -match "(?i)hi.?fi|vb-audio.+cable|echo.?plus|echoaudio|vanysound") {
            $propsKey.DeleteValue($legacyProp, $false)
            Write-Log ("  [Cleanup] [{0}] {1} -> eliminado valor legacy invisible (texto='{2}')" -f $Type, $Guid, $legacyText)
            $propsKey.Dispose()
            return $true
        }

        $propsKey.Dispose()
    } catch {
        Write-Log ("  [Cleanup] [{0}] {1} error limpiando valor legacy: {2}" -f $Type, $Guid, $_) "WARN"
    }

    return $false
}

function Cleanup-LegacyBrokenEndpointLabels {
    param(
        [string[]]$RenderGuids = @(),
        [string[]]$CaptureGuids = @()
    )

    $targets = @()

    if ($RenderGuids.Count -eq 0 -and $CaptureGuids.Count -eq 0) {
        foreach ($type in @("Render", "Capture")) {
            $base = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$type"
            foreach ($dev in @(Get-ChildItem $base -ErrorAction SilentlyContinue)) {
                $targets += [pscustomobject]@{
                    Type = $type
                    Guid = [string]$dev.PSChildName
                }
            }
        }
    } else {
        foreach ($guid in @($RenderGuids | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })) {
            $targets += [pscustomobject]@{ Type = "Render"; Guid = [string]$guid }
        }
        foreach ($guid in @($CaptureGuids | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })) {
            $targets += [pscustomobject]@{ Type = "Capture"; Guid = [string]$guid }
        }
    }

    $changed = $false
    foreach ($target in @($targets | Sort-Object -Property Type, Guid -Unique)) {
        if (Remove-LegacyBrokenEndpointLabel -Type $target.Type -Guid $target.Guid) {
            $changed = $true
        }
    }

    return $changed
}

function Get-HiFiGuidsViaPnP {
    $r = @(); $c = @()
    try {
        $eps = @(Get-PnpDevice -Class AudioEndpoint -PresentOnly -ErrorAction SilentlyContinue |
               Where-Object {
                   $_.InstanceId -match "MMDEVAPI" -and
                   (Test-IsHiFiPnpCandidate $_)
            })
        foreach ($ep in $eps) {
            $m = [Regex]::Match($ep.InstanceId,
                    '\{([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\}$')
            if (-not $m.Success) { continue }
            $guid = "{$($m.Groups[1].Value)}"
            if ($ep.InstanceId -match '0\.0\.0\.') {
                if ($r -notcontains $guid) { $r += $guid }
                Write-Log "  [PnP] Render '$($ep.FriendlyName)' =] $guid"
            } elseif ($ep.InstanceId -match '0\.0\.1\.') {
                if ($c -notcontains $guid) { $c += $guid }
                Write-Log "  [PnP] Capture '$($ep.FriendlyName)' =] $guid"
            }
        }
    } catch { Write-Log "  [PnP] Error: $_ " "WARN" }
    return @{ Render = @($r); Capture = @($c) }
}

# --- Método 2: Scan registro como fallback -----
function Get-HiFiGuidsViaRegistry {
    param([string]$DeviceType)
    $result = @()
    $base = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$DeviceType"
    foreach ($dev in (Get-ChildItem $base -ErrorAction SilentlyContinue)) {
        $guid  = $dev.PSChildName
        $props = Get-ItemProperty "$($dev.PSPath)\Properties" -ErrorAction SilentlyContinue
        if (-not $props) { continue }
        
        $desc = Decode-HiFiPropText $props."{a45c254e-df1c-4efd-8020-67d146a850e0},2"
        $iface = Decode-HiFiPropText $props."{b3f8fa53-0004-438e-9003-51a46e139bfc},6"
        
        if ($desc -and $desc -match "(?i)hi.?fi|vb-audio.+cable|echo.?plus|echoaudio|vanysound") {
            if ($result -notcontains $guid) { $result += $guid }
        } elseif ($iface -and $iface -match "(?i)hi.?fi|vb-audio.+cable|echo.?plus|echoaudio|vanysound") {
            if ($result -notcontains $guid) { $result += $guid }
        }
    }
    return @($result)
}

# --------------------------------------------------------------
# PASO 11a: Asegurar servicios de audio corriendo
# --------------------------------------------------------------
Write-Log "PASO 11: Asegurando servicios de audio y dispositivos habilitados..."
foreach ($svc in @("AudioEndpointBuilder", "Audiosrv")) {
    $s = Get-Service $svc -EA SilentlyContinue
    if ($s -and $s.Status -ne "Running") {
        Write-Log "  Iniciando $svc (estaba: $($s.Status))..."
        Start-Service $svc -EA SilentlyContinue
        Start-Sleep -Seconds 2
    }
}

# --------------------------------------------------------------
# PASO 11b: Re-habilitar dispositivos HiFi Cable deshabilitados
# Cubre: Panel de Sonido (DeviceState=4) y Device Manager (PnP)
# --------------------------------------------------------------
$needsExtraRestart = $false

# 1. PnP layer completa: endpoints + dispositivo raíz/controlador
try {
    $pnpCandidates = Get-PnpDevice -PresentOnly:$false -ErrorAction SilentlyContinue | Where-Object {
        Test-IsHiFiPnpCandidate $_
    }

    foreach ($d in @($pnpCandidates | Sort-Object -Property InstanceId -Unique)) {
        $statusText = [string]$d.Status
        $presentText = [string]$d.Present
        $needsEnable = $false

        if ($statusText -notin @("OK", "Unknown")) {
            $needsEnable = $true
        }
        if ($presentText -eq "False") {
            $needsEnable = $true
        }
        if ($d.InstanceId -match '(?i)VBAudioHFVAIO|ROOT\\MEDIA\\VBAudioHFVAIO') {
            $needsEnable = $true
        }

        if ($needsEnable) {
            $label = if ($d.FriendlyName) { $d.FriendlyName } else { $d.InstanceId }
            Write-Log "  [PnP] Reparando candidato HiFi: $label | status=$statusText | present=$presentText | class=$($d.Class)"
            if (Enable-PnpInstanceRobust -InstanceId $d.InstanceId -Label $label) {
                $needsExtraRestart = $true
            }
        }
    }

    $serviceCandidates = Get-PnpDevice -Class Media -PresentOnly:$false -ErrorAction SilentlyContinue | Where-Object {
        Test-IsHiFiPnpCandidate $_
    }
    foreach ($d in @($serviceCandidates | Sort-Object -Property InstanceId -Unique)) {
        $label = if ($d.FriendlyName) { $d.FriendlyName } else { $d.InstanceId }
        Write-Log "  [Media] Verificando candidato raiz/controlador: $label | status=$($d.Status) | present=$($d.Present)"
        if (Enable-PnpInstanceRobust -InstanceId $d.InstanceId -Label $label) {
            $needsExtraRestart = $true
        }
    }
} catch { Write-Log "  [PnP Enable] Error: $_" "WARN" }

# 2. MMDevices layer: Sound Panel disabled (DeviceState=4 o 8)
$mmBase = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio"
foreach ($type in @("Render", "Capture")) {
    foreach ($dev in (Get-ChildItem "$mmBase\$type" -EA SilentlyContinue)) {
        $devState = try { (Get-ItemProperty $dev.PSPath -EA Stop).DeviceState } catch { continue }
        if ($devState -notin @(4, 8)) { continue }

        $k = Get-Item "$($dev.PSPath)\Properties" -EA SilentlyContinue
        if (-not $k) { continue }
        $isHifi = $false
        foreach ($pn in @("{a45c254e-df1c-4efd-8020-67d146a850e0},2", "{b3f8fa53-0004-438e-9003-51a46e139bfc},6")) {
            $rv = try { $k.GetValue($pn) } catch { $null }
            $nm = if ($rv -is [string]) { $rv } elseif ($rv -is [byte[]] -and $rv.Length -ge 10) {
                try { [System.Text.Encoding]::Unicode.GetString($rv, 8, $rv.Length - 8).TrimEnd([char]0) } catch { $null }
            } else { $null }
            if ($nm -and $nm -match "(?i)hi.?fi|vb-audio|echo.?plus|echoaudio|vanysound") { $isHifi = $true; break }
        }
        if (-not $isHifi) { continue }

        Write-Log "  [MMDev] Habilitando $type\$($dev.PSChildName) (DeviceState=$devState)..."
        if (Test-Path $echoTools) {
            $rp = "HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$type\$($dev.PSChildName)\"
            & $echoTools $rp 2>$null | Out-Null
            Start-Sleep -Milliseconds 300
        }
        try {
            $dKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey(
                "SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$type\$($dev.PSChildName)", $true)
            if ($dKey) {
                $dKey.SetValue("DeviceState", 1, [Microsoft.Win32.RegistryValueKind]::DWord)
                $dKey.Dispose()
                Write-Log "    DeviceState -> 1 (activo)"
                $needsExtraRestart = $true
            }
        } catch { Write-Log "    Error: $_" "WARN" }
    }
}

if ($needsExtraRestart) {
    Write-Log "  Reiniciando AudioEndpointBuilder tras re-habilitar dispositivos..."
    & net stop AudioEndpointBuilder /y 2>$null
    Start-Sleep -Milliseconds 600
    & net start AudioEndpointBuilder 2>$null
    Start-Sleep -Seconds 3
}

# --- Aplicar renombrado nativo de Windows (SetupAPI PnP) PRIMERO ---
Write-Log "PASO 11: Renombrando PnP y configurando 'VanySound' + 48kHz/24bit..."
try {
    Write-Log "Renombrando interfaces PnP base a 'VanySound'..."
    [PnPRenamer]::RenameMediaDevices("VB-Audio Hi-Fi Cable", "VanySound")
    [PnPRenamer]::RenameMediaDevices("Hi-Fi Cable", "VanySound")
    [PnPRenamer]::RenameMediaDevices("Echo PLUS", "VanySound")
    [PnPRenamer]::RenameMediaDevices("EchoAudio", "VanySound")
} catch {
    Write-Log "Error al usar PnPRenamer: $_" "WARN"
}

# Reiniciar AudioEndpointBuilder para forzar generación de nuevos GUIDs de audio
Write-Log "Reiniciando AudioEndpointBuilder y Audiosrv para regenerar endpoints..."
& net stop AudioEndpointBuilder /y 2>$null
Start-Sleep -Seconds 1
& net start AudioEndpointBuilder 2>$null
& net start Audiosrv 2>$null
Start-Sleep -Seconds 4

if (Cleanup-LegacyBrokenEndpointLabels) {
    Write-Log "Se limpiaron valores legacy invisibles antes del escaneo final."
}

# --- Detección de los GUIDs definitivos (después del reinicio del servicio) ---
Write-Log "Detectando GUIDs de HiFi Cable generados..."
$renderGuids  = @()
$captureGuids = @()

for ($i = 0; $i -lt 15; $i++) {
    $pnp = Get-HiFiGuidsViaPnP
    $renderGuids  = Normalize-GuidList $pnp.Render
    $captureGuids = Normalize-GuidList $pnp.Capture

    if ($renderGuids.Count -eq 0)  { $renderGuids  = Normalize-GuidList (Get-HiFiGuidsViaRegistry "Render")  }
    if ($captureGuids.Count -eq 0) { $captureGuids = Normalize-GuidList (Get-HiFiGuidsViaRegistry "Capture") }

    if ($renderGuids.Count -gt 0 -or $captureGuids.Count -gt 0) { break }
    Write-Log "  Intento $($i+1)/15 -- esperando dispositivo definitivo en registro..."
    Start-Sleep -Seconds 1
}

Write-Log "Render GUIDs finales: $($renderGuids -join ', ')"
Write-Log "Capture GUIDs finales: $($captureGuids -join ', ')"

if ($renderGuids.Count -gt 1 -or $captureGuids.Count -gt 1) {
    Write-Log ("WARN: Se detectaron endpoints HiFi duplicados (render={0}, capture={1}). Se intentara deduplicar antes de aplicar VanySound..." -f $renderGuids.Count, $captureGuids.Count) "WARN"
    if (Cleanup-HiFiDuplicates -Reason "post-detection") {
        $pnp = Get-HiFiGuidsViaPnP
        $renderGuids  = Normalize-GuidList $pnp.Render
        $captureGuids = Normalize-GuidList $pnp.Capture
        if ($renderGuids.Count -eq 0)  { $renderGuids  = Normalize-GuidList (Get-HiFiGuidsViaRegistry "Render")  }
        if ($captureGuids.Count -eq 0) { $captureGuids = Normalize-GuidList (Get-HiFiGuidsViaRegistry "Capture") }
        Write-Log "Render GUIDs tras dedup: $($renderGuids -join ', ')"
        Write-Log "Capture GUIDs tras dedup: $($captureGuids -join ', ')"
    }
}

if ($renderGuids.Count -eq 0 -and $captureGuids.Count -eq 0) {
    Write-Log "WARN: No aparecieron endpoints HiFi tras el primer escaneo. Intentando rescate extra de dispositivos..." "WARN"
    try {
        $rescueCandidates = Get-PnpDevice -PresentOnly:$false -ErrorAction SilentlyContinue | Where-Object {
            Test-IsHiFiPnpCandidate $_
        }
        foreach ($d in @($rescueCandidates | Sort-Object -Property InstanceId -Unique)) {
            $label = if ($d.FriendlyName) { $d.FriendlyName } else { $d.InstanceId }
            [void](Enable-PnpInstanceRobust -InstanceId $d.InstanceId -Label $label)
        }
    } catch {
        Write-Log "  [Rescue] Error enumerando candidatos HiFi: $_" "WARN"
    }

    Write-Log "  Reiniciando AudioEndpointBuilder y Audiosrv tras rescate extra..."
    & net stop AudioEndpointBuilder /y 2>$null
    Start-Sleep -Seconds 1
    & net start AudioEndpointBuilder 2>$null
    & net start Audiosrv 2>$null
    Start-Sleep -Seconds 5

    for ($i = 0; $i -lt 10; $i++) {
        $pnp = Get-HiFiGuidsViaPnP
        $renderGuids  = Normalize-GuidList $pnp.Render
        $captureGuids = Normalize-GuidList $pnp.Capture

        if ($renderGuids.Count -eq 0)  { $renderGuids  = Normalize-GuidList (Get-HiFiGuidsViaRegistry "Render")  }
        if ($captureGuids.Count -eq 0) { $captureGuids = Normalize-GuidList (Get-HiFiGuidsViaRegistry "Capture") }

        if ($renderGuids.Count -gt 0 -or $captureGuids.Count -gt 0) { break }
        Write-Log "  [Rescue] Intento $($i+1)/10 -- esperando endpoints HiFi tras rehabiitar..." "WARN"
        Start-Sleep -Seconds 1
    }

    Write-Log "Render GUIDs tras rescate: $($renderGuids -join ', ')"
    Write-Log "Capture GUIDs tras rescate: $($captureGuids -join ', ')"
}

if ($renderGuids.Count -gt 0) {
    if (-not (Test-Path "HKLM:\SOFTWARE\VanySound")) {
        New-Item "HKLM:\SOFTWARE\VanySound" -Force | Out-Null
    }
    New-ItemProperty -Path "HKLM:\SOFTWARE\VanySound" -Name "HiFiEndpointGuid" -Value $renderGuids[0] -PropertyType String -Force | Out-Null
    New-ItemProperty -Path "HKLM:\SOFTWARE\VanySound" -Name "HiFiEndpointName" -Value "VanySound" -PropertyType String -Force | Out-Null
    Write-Log "Endpoint objetivo VanySound guardado: $($renderGuids[0])"
}

if ($renderGuids.Count -eq 0 -and $captureGuids.Count -eq 0) {
    Write-HiFiDiagnostics -Reason "no-endpoints-after-rescue"
    Write-Log "ERROR: HiFi Cable no encontrado en registro incluso tras rescate. No se puede continuar con VanySound sin endpoint objetivo." "ERROR"
    $finalCode = 1
} else {
    foreach ($guid in $renderGuids) {
        [void](Set-MMDeviceStateActive -Type "Render" -Guid $guid)
    }
    foreach ($guid in $captureGuids) {
        [void](Set-MMDeviceStateActive -Type "Capture" -Guid $guid)
    }

    # Bytes del formato 48kHz/24bit (PROPVARIANT VT_BLOB + WAVEFORMATEXTENSIBLE)
    $FMT_BYTES_RENDER_7_1 = [byte[]]($FMT_48K_24BIT_7_1.Split(',') | ForEach-Object { [Convert]::ToByte($_.Trim(), 16) })
    $FMT_BYTES_CAPTURE_STEREO = [byte[]]($FMT_48K_24BIT_STEREO.Split(',') | ForEach-Object { [Convert]::ToByte($_.Trim(), 16) })

    function Apply-EchoPlusReg {
        param([string]$Type, [string]$Guid)
        $regPath  = "HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Type\$Guid\"
        $propsNet = "SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Type\$Guid\Properties"

        # -- EchoTools v2: ownership + nombre --------------------
        if (Test-Path $echoTools) {
            & $echoTools $regPath "VanySound" 2>&1 | ForEach-Object { Write-Log ("  EchoTools: " + $_) }
            Start-Sleep -Milliseconds 500
        }
        [void](Remove-LegacyBrokenEndpointLabel -Type $Type -Guid $Guid)

        # -- .NET Registry API: formato 48kHz/24bit ---------------
        # (mismo mecanismo que EchoTools usa para el nombre -- evita el fallo
        #  silencioso de regedit al escribir REG_SZ sobre REG_BINARY)
        $props = [System.Collections.Generic.List[string]]@(
            "{f19f064d-082c-4e27-bc73-6882a1bb8e4c},0",  # PKEY_AudioEngine_DeviceFormat
            "{e4870e26-3cc5-4cd2-ba46-ca0a9a70ed04},0"   # OEM format
        )
        $fmtBytes = if ($Type -eq "Render") { $FMT_BYTES_RENDER_7_1 } else { $FMT_BYTES_CAPTURE_STEREO }
        if ($Type -eq "Capture") {
            $props.Add("{3d6e1656-2e50-4c4c-8d85-d0acae3c6c68},3")
            $props.Add("{624f56de-fd24-473e-814a-de40aacaed16},3")
        }
        try {
            $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($propsNet, $true)
            if ($key) {
                foreach ($p in $props) {
                    try {
                        $fmtPayload = New-Object byte[] ($fmtBytes.Length)
                        [Array]::Copy($fmtBytes, $fmtPayload, $fmtBytes.Length)
                        try {
                            if ($key.GetValue($p, $null) -ne $null) {
                                $key.DeleteValue($p, $false)
                            }
                        } catch {}
                        $key.SetValue($p, $fmtPayload, [Microsoft.Win32.RegistryValueKind]::Binary)
                        $check = $key.GetValue($p)
                        if ($check -is [byte[]] -and $check.Length -ge 16) {
                            $channels = [BitConverter]::ToUInt16($check, 10)
                            $hz = [BitConverter]::ToUInt32($check, 12)
                            $mask = if ($check.Length -ge 32) { [BitConverter]::ToUInt32($check, 28) } else { 0 }
                            Write-Log ("  [FMT] [{0}] {1} -> {2} Hz / {3}ch / mask=0x{4}" -f $Type, $p, $hz, $channels, $mask.ToString("X"))
                        }
                    } catch {
                        Write-Log ("  [FMT] [{0}] {1} error: {2}" -f $Type, $p, $_) "WARN"
                    }
                }

                if ($Type -eq "Render") {
                    try {
                        $key.SetValue($PKEY_PHYSICAL_SPEAKERS, $SPEAKER_MASK_7_1, [Microsoft.Win32.RegistryValueKind]::DWord)
                        $key.SetValue($PKEY_FULLRANGE_SPEAKERS, $SPEAKER_MASK_7_1, [Microsoft.Win32.RegistryValueKind]::DWord)
                        Write-Log ("  [SPK] [Render] 7.1 surround aplicado. Physical=0x{0} FullRange=0x{0}" -f $SPEAKER_MASK_7_1.ToString("X"))
                    } catch {
                        Write-Log ("  [SPK] [Render] error aplicando 7.1 surround: $_") "WARN"
                    }
                }

                $key.Dispose()
            } else {
                Write-Log ("  [FMT] [$Type] $Guid -> Properties key null") "WARN"
            }
        } catch {
            Write-Log ("  [FMT] [$Type] $Guid error abriendo Properties: $_") "WARN"
        }

        Write-Log ("  " + $Type + " " + $Guid + " -> nombre+formato aplicados" + $(if ($Type -eq "Render") { " (7.1 surround)" } else { "" }))
    }

    # -- Aplicar a cada GUID detectado -------------------------------
    foreach ($guid in $renderGuids) {
        Apply-EchoPlusReg -Type "Render"  -Guid $guid
    }
    foreach ($guid in $captureGuids) {
        Apply-EchoPlusReg -Type "Capture" -Guid $guid
    }

    # -- Reiniciar audio para que cargue los nuevos nombres ----------
    Write-Log "Reiniciando Audiosrv..."
    & net stop Audiosrv /y 2>$null
    Start-Sleep -Milliseconds 800
    & net start Audiosrv 2>$null
    Start-Sleep -Seconds 3

    # Reaplicar tras el reinicio final porque Windows puede regenerar valores
    # del endpoint y sobrescribir formato/estado al volver a levantar Audiosrv.
    Write-Log "Reaplicando estado final tras reiniciar Audiosrv..."
    foreach ($guid in $renderGuids) {
        [void](Set-MMDeviceStateActive -Type "Render" -Guid $guid)
        Apply-EchoPlusReg -Type "Render" -Guid $guid
    }
    foreach ($guid in $captureGuids) {
        [void](Set-MMDeviceStateActive -Type "Capture" -Guid $guid)
        Apply-EchoPlusReg -Type "Capture" -Guid $guid
    }
    Start-Sleep -Seconds 1

    function Get-EndpointFormatInfo {
        param(
            [Parameter(Mandatory = $true)][ValidateSet("Render", "Capture")][string]$Type,
            [Parameter(Mandatory = $true)][string]$Guid
        )

        $propsPath = "SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Type\$Guid\Properties"
        $devicePath = "SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Type\$Guid"
        $speakerMask = 0
        $deviceState = $null
        $bestHz = 0
        $bestChannels = 0

        $propertyNames = @(
            "{f19f064d-082c-4e27-bc73-6882a1bb8e4c},0",
            "{e4870e26-3cc5-4cd2-ba46-ca0a9a70ed04},0"
        )
        if ($Type -eq "Capture") {
            $propertyNames += @(
                "{3d6e1656-2e50-4c4c-8d85-d0acae3c6c68},3",
                "{624f56de-fd24-473e-814a-de40aacaed16},3"
            )
        }

        try {
            $deviceKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($devicePath, $false)
            if ($deviceKey) {
                try { $deviceState = [int]$deviceKey.GetValue("DeviceState", 0) } catch { $deviceState = $null }
                $deviceKey.Dispose()
            }
        } catch {}

        try {
            $propsKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($propsPath, $false)
            if ($propsKey) {
                try { $speakerMask = [uint32]$propsKey.GetValue($PKEY_PHYSICAL_SPEAKERS, 0) } catch { $speakerMask = 0 }
                foreach ($propertyName in $propertyNames) {
                    $fmtRaw = $null
                    try { $fmtRaw = $propsKey.GetValue($propertyName, $null) } catch {}
                    if ($fmtRaw -is [byte[]] -and $fmtRaw.Length -ge 16) {
                        $channels = [BitConverter]::ToUInt16($fmtRaw, 10)
                        $hz = [BitConverter]::ToUInt32($fmtRaw, 12)
                        if ($hz -gt $bestHz -or ($hz -eq $bestHz -and $channels -gt $bestChannels)) {
                            $bestHz = $hz
                            $bestChannels = $channels
                        }
                    }
                }
                $propsKey.Dispose()
            }
        } catch {}

        return [pscustomobject]@{
            Hz = $bestHz
            Channels = $bestChannels
            SpeakerMask = $speakerMask
            DeviceState = $deviceState
        }
    }

    # -- Verificar ---------------------------------------------------
    Write-Log "Verificando resultado final..."
    foreach ($type in @("Render", "Capture")) {
        $guidsToVerify = if ($type -eq "Render") { @($renderGuids) } else { @($captureGuids) }
        foreach ($guid in $guidsToVerify) {
            $formatInfo = Get-EndpointFormatInfo -Type $type -Guid $guid
            $hz = [int]$formatInfo.Hz
            $channels = [int]$formatInfo.Channels
            $speakerMask = [uint32]$formatInfo.SpeakerMask
            $deviceState = if ($null -eq $formatInfo.DeviceState) { "?" } else { [string]$formatInfo.DeviceState }

            Write-Log ("  [OK] [{0}] {1} -> nombre='VanySound' formato={2}Hz canales={3} mask=0x{4} state={5}" -f $type, $guid, $hz, $channels, $speakerMask.ToString("X"), $deviceState)
            Write-Host ("  [OK] [{0}] VanySound @ {1} Hz / {2}ch" -f $type, $hz, $channels) -ForegroundColor Green
            if ($deviceState -ne "1") {
                Write-Log ("  [!!] [{0}] DeviceState sigue en {1}" -f $type, $deviceState) "WARN"
                Write-Host ("  [!!] [{0}] DeviceState sigue en {1}" -f $type, $deviceState) -ForegroundColor Red
            }
            if ($hz -ne 48000) {
                Write-Log ("  [!!] [{0}] Formato sigue en {1} Hz" -f $type, $hz) "WARN"
                Write-Host ("  [!!] [{0}] Formato sigue en {1} Hz" -f $type, $hz) -ForegroundColor Red
            }
            if ($type -eq "Render" -and ($channels -ne 8 -or $speakerMask -ne $SPEAKER_MASK_7_1)) {
                Write-Log ("  [!!] [{0}] 7.1 surround no quedo aplicado. canales={1} mask=0x{2}" -f $type, $channels, $speakerMask.ToString("X")) "WARN"
                Write-Host ("  [!!] [{0}] 7.1 surround no quedo aplicado" -f $type) -ForegroundColor Red
            }
            if ($type -eq "Capture" -and $channels -ne 2) {
                Write-Log ("  [!!] [{0}] Capture no quedo en stereo. canales={1}" -f $type, $channels) "WARN"
                Write-Host ("  [!!] [{0}] Capture no quedo en stereo" -f $type) -ForegroundColor Red
            }
        }
    }

    $finalPnp = Get-HiFiGuidsViaPnP
    $finalRenderVisible = Normalize-GuidList $finalPnp.Render
    $finalCaptureVisible = Normalize-GuidList $finalPnp.Capture
    Write-Log "PnP visible final - Render: $($finalRenderVisible -join ', ')"
    Write-Log "PnP visible final - Capture: $($finalCaptureVisible -join ', ')"

    if ($finalRenderVisible.Count -eq 0 -or $finalCaptureVisible.Count -eq 0) {
        Write-Log ("WARN: Tras la configuracion final siguen faltando endpoints visibles en PnP (render={0}, capture={1}). Forzando un ultimo rescan..." -f $finalRenderVisible.Count, $finalCaptureVisible.Count) "WARN"
        & pnputil /scan-devices 2>$null | ForEach-Object { Write-Log ("  PNPUTIL FINAL SCAN > " + $_) }
        & net stop AudioEndpointBuilder /y 2>$null
        Start-Sleep -Seconds 1
        & net start AudioEndpointBuilder 2>$null
        & net start Audiosrv 2>$null
        Start-Sleep -Seconds 4

        $finalPnp = Get-HiFiGuidsViaPnP
        $finalRenderVisible = Normalize-GuidList $finalPnp.Render
        $finalCaptureVisible = Normalize-GuidList $finalPnp.Capture
        Write-Log "PnP visible tras rescate final - Render: $($finalRenderVisible -join ', ')"
        Write-Log "PnP visible tras rescate final - Capture: $($finalCaptureVisible -join ', ')"
    }

    if ($finalRenderVisible.Count -eq 0 -or $finalCaptureVisible.Count -eq 0) {
        Write-Log ("ERROR: Los endpoints HiFi no quedaron visibles al final (render={0}, capture={1})." -f $finalRenderVisible.Count, $finalCaptureVisible.Count) "ERROR"
        $finalCode = 1
    } else {
        # Rescan recovered — clear any prior error
        $finalCode = 0
    }
    if ($finalRenderVisible.Count -gt 1 -or $finalCaptureVisible.Count -gt 1) {
        Write-Log ("ERROR: Siguen existiendo endpoints HiFi duplicados al final (render={0}, capture={1})." -f $finalRenderVisible.Count, $finalCaptureVisible.Count) "ERROR"
        $finalCode = 1
    }

    Write-Log "Servicios de audio listos."
}


Write-Host ("`n    Log completo: " + $LOG_FILE)
exit $finalCode
