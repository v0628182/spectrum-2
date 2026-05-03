#Requires -Version 5.1
<#
.SYNOPSIS
    Activa Loudness Equalization en TODOS los dispositivos de audio Render activos.
    100% silencioso -- sin ventanas, sin parámetros obligatorios.

.DESCRIPTION
    INGENIERÍA INVERSA del script enable-loudness-equalisation (github.com/Falcosc).
    Correcciones vs el original:
      - Sin parámetros Mandatory (no aparece prompt interactivo)
      - Sin MessageBox (no abre ventanas)
      - Auto-elevación silenciosa ANTES de cualquier operación
      - Compatible con Windows 10/11 x64 y ARM64
      - Soporta dispositivos donde FxProperties no existe (los crea)
      - Aplica a TODOS los dispositivos Render activos ó solo HiFi Cable si se especifica
      - Reinicia AudioSrv silenciosamente

.LINK
    https://github.com/Falcosc/enable-loudness-equalisation
#>

param(
    [switch]$ConsoleLog,
    [switch]$SkipSelfElevation
)

Set-StrictMode -Off   # Off en vez de Latest para evitar errores en props nulas
$ErrorActionPreference = "SilentlyContinue"

$LOG_FILE = Join-Path $env:TEMP "vanysound_loudness.log"

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

# ==============================================================
# AUTO-ELEVACIÓN SILENCIOSA (antes de todo)
# ==============================================================
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
    Start-Process powershell.exe `
        -ArgumentList ($elevatedArgs -join " ") `
        -Verb RunAs -Wait -WindowStyle $elevatedWindowStyle
    exit
}

Write-Log "=== INICIO Loudness Equalization ==="
Write-Log "LOUDNESS LOG: $LOG_FILE"
Write-Log "OS: $([System.Environment]::OSVersion.VersionString)"

# ==============================================================
# CONFIGURACIÓN
# ==============================================================

# releaseTime: 2 (rápido) a 7 (lento) -- 4 es el valor por defecto de Windows
$releaseTime = 4

$targetNamePattern = "(?i)vanysound|echoaudio|echo plus|hi.?fi|vb-audio.+cable"

# ==============================================================
# CLAVES DE REGISTRO (sacadas del script original + research)
# ==============================================================
$KEY_ENHANCEMENT_FLAG = "{fc52a749-4be9-4510-896e-966ba6525980},3"
$KEY_RELEASE_TIME     = "{9c00eeed-edce-4cd8-ae08-cb05e8ef57a0},3"
$KEY_UI_CLSID         = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},3"
$VAL_UI_CLSID         = "{5860E1C5-F95C-4a7a-8EC8-8AEF24F379A1}"

$installedHelper = "C:\Program Files\VanySoundEngine\VanySoundControl.exe"
$localHelper = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Definition) "VanySoundControl.exe"

$enhancementBytes = [byte[]](0x0b, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00)
$releaseBytes = [byte[]](0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, [byte]$releaseTime, 0x00, 0x00, 0x00)

function Decode-RegistryText {
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

function Get-DeviceDisplayName {
    param([string]$Guid)

    $propPath = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$Guid\Properties"
    try {
        $props = Get-ItemProperty $propPath -ErrorAction SilentlyContinue
        if (-not $props) { return $Guid }

        foreach ($name in $props.PSObject.Properties.Name) {
            if ($name -like "PS*") { continue }
            $decoded = Decode-RegistryText $props.$name
            if (-not [string]::IsNullOrWhiteSpace($decoded)) {
                if ($decoded -match "(?i)vanysound|echoaudio|echo plus|hi.?fi|vb-audio") {
                    return $decoded
                }
            }
        }
    } catch {}

    return $Guid
}

function Test-IsHiFiLikeRenderDevice {
    param([string]$Guid)

    $name = Get-DeviceDisplayName -Guid $Guid
    return ($name -match $targetNamePattern)
}

function Test-LoudnessState {
    param([string]$Guid)

    $fxPath = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$Guid\FxProperties"
    $fxProps = Get-ItemProperty $fxPath -ErrorAction SilentlyContinue
    if (-not $fxProps) {
        return $false
    }

    $flagOk = $fxProps.$KEY_ENHANCEMENT_FLAG -is [byte[]] -and
        $fxProps.$KEY_ENHANCEMENT_FLAG.Length -ge 10 -and
        $fxProps.$KEY_ENHANCEMENT_FLAG[8] -eq 0xff -and
        $fxProps.$KEY_ENHANCEMENT_FLAG[9] -eq 0xff

    $releaseOk = $fxProps.$KEY_RELEASE_TIME -is [byte[]] -and
        $fxProps.$KEY_RELEASE_TIME.Length -ge 9 -and
        $fxProps.$KEY_RELEASE_TIME[8] -eq $releaseTime

    $tabOk = [string]$fxProps.$KEY_UI_CLSID -eq $VAL_UI_CLSID

    return ($flagOk -and $releaseOk -and $tabOk)
}

function Set-LoudnessState {
    param([string]$Guid)

    $fxPath = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$Guid\FxProperties"

    # Take ownership and grant admin write access (MMDevice keys have restrictive ACLs)
    $parentPath = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$Guid"
    Grant-RegistryKeyAccess -RegPath $parentPath
    Grant-RegistryKeyAccess -RegPath $fxPath

    if (-not (Test-Path $fxPath)) {
        New-Item -Path $fxPath -Force | Out-Null
    }

    New-ItemProperty -Path $fxPath -Name $KEY_UI_CLSID -Value $VAL_UI_CLSID -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $fxPath -Name $KEY_ENHANCEMENT_FLAG -Value $enhancementBytes -PropertyType Binary -Force | Out-Null
    New-ItemProperty -Path $fxPath -Name $KEY_RELEASE_TIME -Value $releaseBytes -PropertyType Binary -Force | Out-Null
}

function Grant-RegistryKeyAccess {
    param([string]$RegPath)

    # First try EchoTools.exe (most reliable for MMDevice keys)
    $echoToolsPath = Join-Path $PSScriptRoot "EchoTools.exe"
    if (Test-Path $echoToolsPath) {
        $netPath = $RegPath -replace '^HKLM:\\', 'HKEY_LOCAL_MACHINE\'
        & $echoToolsPath $netPath 2>$null | Out-Null
        Start-Sleep -Milliseconds 200
        Write-Log "  ACL: used EchoTools for $RegPath"
        return
    }

    # Fallback: PowerShell ACL adjustment with privilege escalation
    try {
        if (-not (Test-Path $RegPath)) { return }

        # Enable SeTakeOwnershipPrivilege
        $tokenPriv = @"
using System;
using System.Runtime.InteropServices;
public class TokenPriv {
    [DllImport("advapi32.dll", SetLastError=true)]
    static extern bool OpenProcessToken(IntPtr ProcessHandle, uint DesiredAccess, out IntPtr TokenHandle);
    [DllImport("advapi32.dll", SetLastError=true)]
    static extern bool LookupPrivilegeValue(string lpSystemName, string lpName, out long lpLuid);
    [DllImport("advapi32.dll", SetLastError=true)]
    static extern bool AdjustTokenPrivileges(IntPtr TokenHandle, bool DisableAllPrivileges, ref TOKEN_PRIVILEGES NewState, int BufferLength, IntPtr PreviousState, IntPtr ReturnLength);
    [StructLayout(LayoutKind.Sequential)] public struct TOKEN_PRIVILEGES { public int PrivilegeCount; public long Luid; public int Attributes; }
    public static void Enable(string priv) {
        IntPtr token;
        OpenProcessToken((IntPtr)(-1), 0x28, out token);
        TOKEN_PRIVILEGES tp = new TOKEN_PRIVILEGES { PrivilegeCount = 1, Attributes = 2 };
        LookupPrivilegeValue(null, priv, out tp.Luid);
        AdjustTokenPrivileges(token, false, ref tp, 0, IntPtr.Zero, IntPtr.Zero);
    }
}
"@
        try { Add-Type $tokenPriv -ErrorAction SilentlyContinue } catch {}
        [TokenPriv]::Enable("SeTakeOwnershipPrivilege")
        [TokenPriv]::Enable("SeRestorePrivilege")

        # Open key with TakeOwnership permission
        $hiveKey = [Microsoft.Win32.Registry]::LocalMachine
        $subPath = $RegPath -replace '^HKLM:\\', ''
        $key = $hiveKey.OpenSubKey($subPath,
            [Microsoft.Win32.RegistryKeyPermissionCheck]::ReadWriteSubTree,
            [System.Security.AccessControl.RegistryRights]::TakeOwnership)

        if ($key) {
            $acl = $key.GetAccessControl()
            $adminSid = New-Object System.Security.Principal.SecurityIdentifier("S-1-5-32-544")
            $acl.SetOwner($adminSid)
            $key.SetAccessControl($acl)
            $key.Close()

            # Now reopen with full control to add the ACE
            $key = $hiveKey.OpenSubKey($subPath,
                [Microsoft.Win32.RegistryKeyPermissionCheck]::ReadWriteSubTree,
                [System.Security.AccessControl.RegistryRights]::ChangePermissions)
            $acl = $key.GetAccessControl()
            $rule = New-Object System.Security.AccessControl.RegistryAccessRule(
                $adminSid,
                [System.Security.AccessControl.RegistryRights]::FullControl,
                [System.Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [System.Security.AccessControl.InheritanceFlags]::ObjectInherit,
                [System.Security.AccessControl.PropagationFlags]::None,
                [System.Security.AccessControl.AccessControlType]::Allow
            )
            $acl.AddAccessRule($rule)
            $key.SetAccessControl($acl)
            $key.Close()
            Write-Log "  ACL: took ownership and granted admin access to $RegPath"
        } else {
            Write-Log "  ACL: could not open key for ownership: $RegPath" "WARN"
        }
    } catch {
        Write-Log "  ACL: failed to adjust permissions for $RegPath : $_" "WARN"
    }
}

function Resolve-ControlHelperPath {
    foreach ($candidate in @($installedHelper, $localHelper)) {
        if (-not [string]::IsNullOrWhiteSpace($candidate) -and (Test-Path $candidate)) {
            return $candidate
        }
    }

    return $null
}

function Resolve-NativeAppPath {
    $scriptRoot = Split-Path -Parent $MyInvocation.ScriptName
    $parentDir = [System.IO.Path]::GetFullPath((Join-Path $scriptRoot ".."))
    $installDir = "C:\Program Files\VanySound"
    $exeNames = @("vanysound-app.exe", "VanySound.exe", "app3.exe")
    foreach ($dir in @($installDir, $parentDir, $scriptRoot)) {
        foreach ($name in $exeNames) {
            $candidate = Join-Path $dir $name
            if (Test-Path $candidate) {
                return $candidate
            }
        }
    }
    return $null
}

function Invoke-ControlHelper {
    param([string[]]$Arguments)

    # Prefer the native app with embedded CLI
    $nativeApp = Resolve-NativeAppPath
    if ($nativeApp) {
        $output = & $nativeApp "__vanysound_native__" @Arguments 2>&1
        $exitCode = $LASTEXITCODE
        foreach ($line in @($output)) {
            if (-not [string]::IsNullOrWhiteSpace($line)) {
                Write-Log "CONTROL $($Arguments -join ' ') :: $line"
            }
        }
        Write-Log "CONTROL $($Arguments -join ' ') exit=$exitCode (native=$nativeApp)"
        return ($exitCode -eq 0)
    }

    # Fallback to standalone helper
    $helper = Resolve-ControlHelperPath
    if (-not $helper) {
        Write-Log "WARN: No control helper found (native app or VanySoundControl.exe)." "WARN"
        return $false
    }

    $output = & $helper @Arguments 2>&1
    $exitCode = $LASTEXITCODE
    foreach ($line in @($output)) {
        if (-not [string]::IsNullOrWhiteSpace($line)) {
            Write-Log "CONTROL $($Arguments -join ' ') :: $line"
        }
    }
    Write-Log "CONTROL $($Arguments -join ' ') exit=$exitCode"
    return ($exitCode -eq 0)
}

# ==============================================================
# OBTENER DISPOSITIVO OBJETIVO
# ==============================================================
$renderRoot = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render"
$echoRoot = "HKLM:\SOFTWARE\VanySound"

$allDevices = Get-ChildItem $renderRoot -ErrorAction SilentlyContinue

if (-not $allDevices -or $allDevices.Count -eq 0) {
    Write-Log "ERROR: No se puede leer MMDevices. ¿Script sin admin?" "ERROR"
    exit 1
}

$targetGuids = New-Object System.Collections.Generic.List[string]
try {
    $echoState = Get-ItemProperty $echoRoot -ErrorAction SilentlyContinue
    if ($echoState -and $echoState.HiFiEndpointGuid) {
        $storedGuid = [string]$echoState.HiFiEndpointGuid
        if (-not [string]::IsNullOrWhiteSpace($storedGuid) -and (Test-Path (Join-Path $renderRoot $storedGuid))) {
            $targetGuids.Add($storedGuid)
            Write-Log "Usando endpoint objetivo de VanySound: $storedGuid ($($echoState.HiFiEndpointName))"
        }
    }
} catch {}

if ($targetGuids.Count -eq 0) {
    $matchingDevices = $allDevices | Where-Object {
        try {
            $state = (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).DeviceState
            ($state -eq 1) -and (Test-IsHiFiLikeRenderDevice -Guid $_.PSChildName)
        } catch { $false }
    }

    Write-Log "Fallback: endpoints HiFi/VanySound activos encontrados: $($matchingDevices.Count)"
    foreach ($dev in $matchingDevices) {
        $targetGuids.Add($dev.PSChildName)
    }
}

if ($targetGuids.Count -eq 0) {
    Write-Log "ERROR: Ningun endpoint VanySound/Hi-Fi valido disponible para loudness. No se tocara otro dispositivo de audio." "ERROR"
    exit 1
}

# ==============================================================
# APLICAR LOUDNESS AL ENDPOINT OBJETIVO
# ==============================================================
$modified    = 0
$skipped     = 0
$failed      = 0

foreach ($guid in $targetGuids) {
    $devName = Get-DeviceDisplayName -Guid $guid

    if (Test-LoudnessState -Guid $guid) {
        Write-Log "  OK (ya activo): '$devName'"
        $skipped++
        continue
    }

    Write-Log "  APLICANDO: '$devName' ($guid)"
    try {
        Set-LoudnessState -Guid $guid
        if (Test-LoudnessState -Guid $guid) {
            Write-Log "  OK (aplicado): '$devName'"
            $modified++
        } else {
            Write-Log "  ERROR: verificacion post-escritura fallo para '$devName'" "ERROR"
            $failed++
        }
    } catch {
        Write-Log "  ERROR: no se pudo aplicar loudness a '$devName' =] $_" "ERROR"
        $failed++
    }
}

if ($modified -eq 0 -and $failed -eq 0) {
    Write-Log "Loudness ya activo en todos los endpoints objetivo (skipped=$skipped)."
}

# ==============================================================
# REINICIAR SERVICIO DE AUDIO
# ==============================================================
Write-Log "Reiniciando AudioSrv..."
try {
    & net stop Audiosrv /y 2>$null
    Start-Sleep -Milliseconds 800
    & net start Audiosrv 2>$null
    Write-Log "AudioSrv reiniciado OK."
} catch {
    Write-Log "WARN al reiniciar AudioSrv: $_ (puede requerir reboot)" "WARN"
}

# Give Windows time to rebuild audio endpoint handles before reading FxProperties
Start-Sleep -Seconds 3

$verifyDeadline = (Get-Date).AddSeconds(15)
foreach ($guid in $targetGuids) {
    if ((Get-Date) -gt $verifyDeadline) {
        Write-Log "  VERIFY TIMEOUT: exceeded post-restart verification deadline" "WARN"
        break
    }
    $devName = Get-DeviceDisplayName -Guid $guid
    if (Test-LoudnessState -Guid $guid) {
        Write-Log "  VERIFY OK: '$devName'"
    } else {
        Write-Log "  VERIFY FAIL: '$devName' sigue sin loudness activo" "ERROR"
        $failed++
    }
}

if ($failed -gt 0) {
    Write-Log "=== FIN | Loudness con fallos | modified=$modified skipped=$skipped failed=$failed ===" "ERROR"
    exit 1
}

Write-Log "=== FIN | Loudness activo | modified=$modified skipped=$skipped ==="
exit 0
