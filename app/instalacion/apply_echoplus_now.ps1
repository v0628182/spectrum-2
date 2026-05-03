#Requires -Version 5.1
# apply_echoplus_now.ps1 v4
# Autosuficiente: maneja cualquier estado del sistema
#   - Servicios parados -> los arranca
#   - Dispositivo deshabilitado en Panel de Sonido (DeviceState=4) -> lo habilita
#   - Dispositivo deshabilitado en Device Manager (PnP) -> Enable-PnpDevice
#   - EchoTools: ownership + nombre (BINARY/STRING segun tipo actual)
#   - .NET Registry API: formato 48kHz/24bit

Set-StrictMode -Off
$ErrorActionPreference = "SilentlyContinue"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

# ── Auto-elevacion ──────────────────────────────────────────────
$esAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $esAdmin) {
    Start-Process powershell.exe -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$($MyInvocation.MyCommand.Definition)`"" -Verb RunAs -Wait
    exit
}

$echoTools = Join-Path $scriptDir "EchoTools.exe"
if (-not (Test-Path $echoTools)) {
    Write-Host "[X] EchoTools.exe no encontrado en $scriptDir" -ForegroundColor Red; exit 1
}

# ── Formato WAVEFORMATEXTENSIBLE 48000Hz / 24bit / 2ch ─────────
$FMT_HEX   = "41,00,00,00,01,00,00,00,fe,ff,02,00,80,bb,00,00,00,65,04,00,06,00,18,00,16,00,18,00,03,00,00,00,01,00,00,00,00,00,10,00,80,00,00,aa,00,38,9b,71"
$FMT_BYTES = [byte[]]($FMT_HEX.Split(',') | ForEach-Object { [Convert]::ToByte($_, 16) })

function Decode-PV($raw) {
    if ($raw -is [string]) { return $raw }
    if ($raw -is [byte[]] -and $raw.Length -ge 10) {
        try {
            $vt = [BitConverter]::ToUInt16($raw, 0)
            if ($vt -eq 31) { return [System.Text.Encoding]::Unicode.GetString($raw, 8, $raw.Length - 8).TrimEnd([char]0) }
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
        $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($propsPath, $true)
        if (-not $key) { return $false }

        $legacyRaw = $null
        try { $legacyRaw = $key.GetValue($legacyProp, $null) } catch {}
        if ($null -eq $legacyRaw) {
            $key.Dispose()
            return $false
        }

        $legacyText = Decode-PV $legacyRaw
        $descText = Decode-PV ($key.GetValue("{a45c254e-df1c-4efd-8020-67d146a850e0},2", $null))
        $ifaceText = Decode-PV ($key.GetValue("{b3f8fa53-0004-438e-9003-51a46e139bfc},6", $null))
        $joined = @($legacyText, $descText, $ifaceText) -join " | "

        if ($joined -match "(?i)hi.?fi|vb-audio.+cable|echo.?plus|echoaudio|vanysound") {
            $key.DeleteValue($legacyProp, $false)
            Write-Host "  [Cleanup] [$Type] $Guid -> eliminado valor legacy invisible" -ForegroundColor Green
            $key.Dispose()
            return $true
        }

        $key.Dispose()
    } catch {
        Write-Host "  [!] [$Type] $Guid error limpiando valor legacy: $_" -ForegroundColor Red
    }

    return $false
}

function Cleanup-LegacyBrokenEndpointLabels {
    $changed = $false
    $mmBase = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio"

    foreach ($type in @("Render", "Capture")) {
        foreach ($dev in @(Get-ChildItem "$mmBase\$type" -EA SilentlyContinue)) {
            if (Remove-LegacyBrokenEndpointLabel -Type $type -Guid $dev.PSChildName) {
                $changed = $true
            }
        }
    }

    return $changed
}

# ═══════════════════════════════════════════════════════════════
# PASO 0: Asegurar que los servicios de audio esten corriendo
# ═══════════════════════════════════════════════════════════════
Write-Host "`n[*] Asegurando servicios de audio..." -ForegroundColor Cyan

foreach ($svc in @("AudioEndpointBuilder", "Audiosrv")) {
    $s = Get-Service $svc -EA SilentlyContinue
    if (-not $s) { continue }
    if ($s.Status -ne "Running") {
        Write-Host "  Iniciando $svc (estaba: $($s.Status))..." -ForegroundColor Yellow
        Start-Service $svc -EA SilentlyContinue
        Start-Sleep -Seconds 2
        $s.Refresh()
        Write-Host "  $svc -> $($s.Status)" -ForegroundColor $(if($s.Status -eq "Running"){"Green"}else{"Red"})
    } else {
        Write-Host "  $svc -> Running OK" -ForegroundColor Green
    }
}

# ═══════════════════════════════════════════════════════════════
# PASO 1: Re-habilitar dispositivos HiFi Cable deshabilitados
# Escenario: usuario deshabilitó desde Panel de Sonido (DeviceState=4)
#            o desde Device Manager (PnP disabled)
# ═══════════════════════════════════════════════════════════════
Write-Host "`n[*] Verificando dispositivos deshabilitados..." -ForegroundColor Cyan

$needsRestart = $false

if (Cleanup-LegacyBrokenEndpointLabels) {
    $needsRestart = $true
}

# 1A. PnP layer: Device Manager disabled
# AudioEndpoint class devices que no esten en estado OK
$pnpDevs = Get-PnpDevice -EA SilentlyContinue | Where-Object {
    $_.InstanceId -match "MMDEVAPI" -and $_.Status -notin @("OK","Unknown")
}
foreach ($d in $pnpDevs) {
    if ($d.FriendlyName -match "(?i)hi.?fi|vb-audio|echo.?plus|echoaudio|vanysound|hifvaio") {
        Write-Host "  [PnP] Habilitando: $($d.FriendlyName) (PnP Status=$($d.Status))" -ForegroundColor Yellow
        Enable-PnpDevice -InstanceId $d.InstanceId -Confirm:$false -EA SilentlyContinue
        $needsRestart = $true
    }
}

# 1B. MMDevices layer: Panel de Sonido disabled (DeviceState=4 o 8)
# DeviceState: 1=activo, 4=deshabilitado, 8=no_presente
$mmBase = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio"
foreach ($type in @("Render", "Capture")) {
    foreach ($dev in (Get-ChildItem "$mmBase\$type" -EA SilentlyContinue)) {
        $state = try { (Get-ItemProperty $dev.PSPath -EA Stop).DeviceState } catch { continue }
        if ($state -notin @(4, 8)) { continue }  # solo los deshabilitados

        # Verificar si es HiFi Cable leyendo Properties (disponibles aunque este deshabilitado)
        $k = Get-Item "$($dev.PSPath)\Properties" -EA SilentlyContinue
        if (-not $k) { continue }

        $isHifi = $false
        foreach ($pn in @(
            "{a45c254e-df1c-4efd-8020-67d146a850e0},2",
            "{b3f8fa53-0004-438e-9003-51a46e139bfc},6"
        )) {
            $s = Decode-PV (try { $k.GetValue($pn) } catch { $null })
            if ($s -and $s -match "(?i)hi.?fi|vb-audio|echo.?plus|echoaudio|vanysound") {
                $isHifi = $true; break
            }
        }
        if (-not $isHifi) { continue }

        Write-Host "  [MMDev] Habilitando $type\$($dev.PSChildName) (DeviceState=$state, nombre=$(Decode-PV($k.GetValue('{a45c254e-df1c-4efd-8020-67d146a850e0},2'))))" -ForegroundColor Yellow

        # EchoTools: tomar ownership del device root
        $regPath = "HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$type\$($dev.PSChildName)\"
        & $echoTools $regPath 2>$null | Out-Null
        Start-Sleep -Milliseconds 300

        # Escribir DeviceState=1 via .NET Registry API
        try {
            $devKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey(
                "SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$type\$($dev.PSChildName)", $true)
            if ($devKey) {
                $devKey.SetValue("DeviceState", 1, [Microsoft.Win32.RegistryValueKind]::DWord)
                $devKey.Dispose()
                Write-Host "    DeviceState -> 1 (activo)" -ForegroundColor Green
                $needsRestart = $true
            }
        } catch {
            Write-Host "    Error habilitando: $_" -ForegroundColor Red
        }
    }
}

# Si habilitamos algun dispositivo, reiniciar AudioEndpointBuilder para regenerar endpoints
if ($needsRestart) {
    Write-Host "  Reiniciando AudioEndpointBuilder para regenerar endpoints..." -ForegroundColor Yellow
    & net stop AudioEndpointBuilder /y 2>$null
    Start-Sleep -Milliseconds 600
    & net start AudioEndpointBuilder 2>$null
    Start-Sleep -Seconds 4
    Write-Host "  Endpoints regenerados." -ForegroundColor Green
} else {
    Write-Host "  Todos los dispositivos ya estaban activos." -ForegroundColor Green
}

# ═══════════════════════════════════════════════════════════════
# PASO 2: Detectar GUIDs de HiFi Cable (incluyendo recien habilitados)
# ═══════════════════════════════════════════════════════════════
Write-Host "`n[*] Buscando dispositivos HiFi Cable..." -ForegroundColor Cyan

function Find-HiFiGuids {
    param([string]$Type)
    $base  = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Type"
    $found = @()
    foreach ($dev in (Get-ChildItem $base -EA SilentlyContinue)) {
        $k = Get-Item "$($dev.PSPath)\Properties" -EA SilentlyContinue
        if (-not $k) { continue }
        foreach ($pn in @(
            "{a45c254e-df1c-4efd-8020-67d146a850e0},2",
            "{b3f8fa53-0004-438e-9003-51a46e139bfc},6"
        )) {
            $raw = try { $k.GetValue($pn) } catch { $null }
            $s   = Decode-PV $raw
            if ($s -and $s -match "(?i)hi.?fi|vb-audio.+cable|echo.?plus|echoaudio|vanysound") {
                $found += @{ Guid = $dev.PSChildName; Name = $s }
                Write-Host "  Encontrado [$Type]: $s -> $($dev.PSChildName)" -ForegroundColor Green
                break
            }
        }
    }
    return $found
}

$renderDevs  = Find-HiFiGuids "Render"
$captureDevs = Find-HiFiGuids "Capture"

if ($renderDevs.Count -eq 0 -and $captureDevs.Count -eq 0) {
    Write-Host "`n[X] No se encontro HiFi Cable. Asegurate de que el driver este instalado." -ForegroundColor Red
    exit 1
}

# ═══════════════════════════════════════════════════════════════
# PASO 3: EchoTools v2 — ownership + nombre "VanySound"
# ═══════════════════════════════════════════════════════════════
Write-Host "`n[*] EchoTools: tomando ownership y escribiendo nombre..." -ForegroundColor Cyan

foreach ($dev in ($renderDevs + $captureDevs)) {
    $guid = $dev.Guid
    foreach ($type in @("Render", "Capture")) {
        if (Test-Path "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$type\$guid") {
            $regPath = "HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$type\$guid\"
            Write-Host "  -> $type\$guid" -ForegroundColor DarkCyan
            & $echoTools $regPath "VanySound"
            [void](Remove-LegacyBrokenEndpointLabel -Type $type -Guid $guid)
        }
    }
}
Start-Sleep -Milliseconds 500

# ═══════════════════════════════════════════════════════════════
# PASO 4: .NET Registry API — formato 48kHz/24bit
# ═══════════════════════════════════════════════════════════════
Write-Host "`n[*] Aplicando formato 48kHz/24bit..." -ForegroundColor Cyan

function Write-AudioFormat {
    param([string]$Type, [hashtable]$Dev, [byte[]]$Bytes)
    $guid = $Dev.Guid
    $path = "SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$Type\$guid\Properties"

    $props = @(
        "{f19f064d-082c-4e27-bc73-6882a1bb8e4c},0",
        "{e4870e26-3cc5-4cd2-ba46-ca0a9a70ed04},0"
    )
    if ($Type -eq "Capture") {
        $props += "{3d6e1656-2e50-4c4c-8d85-d0acae3c6c68},3"
        $props += "{624f56de-fd24-473e-814a-de40aacaed16},3"
    }

    try {
        $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($path, $true)
        if (-not $key) {
            Write-Host "  [!] [$Type] Properties no encontrada" -ForegroundColor Red; return
        }
        foreach ($p in $props) {
            try {
                $key.SetValue($p, $Bytes, [Microsoft.Win32.RegistryValueKind]::Binary)
                $check = $key.GetValue($p)
                if ($check -is [byte[]] -and $check.Length -ge 16) {
                    $hz = [BitConverter]::ToUInt32($check, 12)
                    $col = if ($hz -eq 48000) { "Green" } else { "Yellow" }
                    Write-Host "  [$(if($hz-eq 48000){'OK'}else{'!!'})] [$Type] formato -> $hz Hz" -ForegroundColor $col
                }
            } catch {
                Write-Host "  [!] Error en $p : $_" -ForegroundColor Red
            }
        }
        $key.Dispose()
    } catch {
        Write-Host "  [!] Error abriendo Properties: $_" -ForegroundColor Red
    }
}

foreach ($dev in $renderDevs)  { Write-AudioFormat "Render"  $dev $FMT_BYTES }
foreach ($dev in $captureDevs) { Write-AudioFormat "Capture" $dev $FMT_BYTES }

# ═══════════════════════════════════════════════════════════════
# PASO 5: Reiniciar Audiosrv para aplicar cambios
# ═══════════════════════════════════════════════════════════════
Write-Host "`n[*] Reiniciando Audiosrv..." -ForegroundColor Cyan
& net stop Audiosrv /y 2>$null
Start-Sleep -Milliseconds 800
& net start Audiosrv 2>$null
Start-Sleep -Seconds 2

# ═══════════════════════════════════════════════════════════════
# VERIFICACION FINAL
# ═══════════════════════════════════════════════════════════════
Write-Host "`n=== VERIFICACION FINAL ===" -ForegroundColor Cyan
$allOk = $true
foreach ($type in @("Render","Capture")) {
    $base = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\$type"
    foreach ($dev in (Get-ChildItem $base -EA SilentlyContinue)) {
        $k = Get-Item "$($dev.PSPath)\Properties" -EA SilentlyContinue
        if (-not $k) { continue }
        $nameRaw = try { $k.GetValue("{a45c254e-df1c-4efd-8020-67d146a850e0},2") } catch { $null }
        $name = Decode-PV $nameRaw
        if ($name -ne "VanySound") { continue }

        $fmtRaw = try { $k.GetValue("{f19f064d-082c-4e27-bc73-6882a1bb8e4c},0") } catch { $null }
        $hz = 0
        if ($fmtRaw -is [byte[]] -and $fmtRaw.Length -ge 16) {
            $hz = [BitConverter]::ToUInt32($fmtRaw, 12)
        }

        $fmtTxt = if ($hz -eq 48000) { "48000 Hz [OK]" } elseif ($hz -gt 0) { "$hz Hz [!!]"; $allOk=$false } else { "(STRING, verificar manualmente)" }
        Write-Host "  [OK] [$type] nombre='VanySound' | formato=$fmtTxt" -ForegroundColor Green
    }
}

if ($allOk) {
    Write-Host "`n[OK] VanySound configurado correctamente." -ForegroundColor Green
} else {
    Write-Host "`n[!!] Algunas propiedades no se aplicaron. Revisa el log." -ForegroundColor Yellow
}
Write-Host ""
