param(
    [switch]$NoBuild,
    [string]$TargetDir = "target-gnullvm",
    [string]$OutputDir = "dist\EchoAudio"
)

$ErrorActionPreference = "Stop"

function Get-LlvmCargo {
    $rustBins = @(
        "C:\Program Files\Rust stable LLVM 1.94\bin"
    )

    foreach ($bin in $rustBins) {
        $candidate = Join-Path $bin "cargo.exe"
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    throw "No se encontro cargo.exe del toolchain LLVM."
}

function Get-MingwBin {
    if ($env:Path) {
        $pathEntry = ($env:Path -split ";") |
            Where-Object { $_ -like "*LLVM-MinGW*bin" } |
            Select-Object -First 1
        if ($pathEntry) {
            return $pathEntry
        }
    }

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $pathEntry = ($userPath -split ";") |
        Where-Object { $_ -like "*LLVM-MinGW*bin" } |
        Select-Object -First 1
    if ($pathEntry) {
        return $pathEntry
    }

    throw "No se encontro LLVM-MinGW en el PATH."
}

function Get-ImportedDllNames {
    param(
        [string]$BinaryPath,
        [string]$ObjdumpPath
    )

    & $ObjdumpPath -p $BinaryPath |
        Select-String "DLL Name:" |
        ForEach-Object {
            if ($_.Line -match "DLL Name:\s+(.+)$") {
                $matches[1].Trim()
            }
        } |
        Sort-Object -Unique
}

function Get-RuntimeDllPaths {
    param(
        [string]$BinaryPath,
        [string]$MingwBin,
        [string]$ObjdumpPath
    )

    $resolved = New-Object "System.Collections.Generic.Dictionary[string,string]" ([System.StringComparer]::OrdinalIgnoreCase)
    $pending = New-Object System.Collections.Generic.Queue[string]
    $pending.Enqueue((Resolve-Path $BinaryPath).Path)

    while ($pending.Count -gt 0) {
        $current = $pending.Dequeue()
        foreach ($dllName in Get-ImportedDllNames -BinaryPath $current -ObjdumpPath $ObjdumpPath) {
            $candidate = Join-Path $MingwBin $dllName
            if (-not (Test-Path $candidate)) {
                continue
            }

            if (-not $resolved.ContainsKey($dllName)) {
                $fullPath = (Resolve-Path $candidate).Path
                $resolved[$dllName] = $fullPath
                $pending.Enqueue($fullPath)
            }
        }
    }

    return $resolved.Values | Sort-Object
}

function Copy-ReleaseArtifacts {
    param(
        [string]$ExePath,
        [string[]]$RuntimeDlls,
        [string]$DestinationDir
    )

    New-Item -ItemType Directory -Force -Path $DestinationDir | Out-Null
    $destinationExe = Join-Path $DestinationDir "echo-audio.exe"
    $sourceExe = (Resolve-Path $ExePath).Path
    if ($sourceExe -ne $destinationExe) {
        Copy-Item -LiteralPath $ExePath -Destination $destinationExe -Force
    }

    foreach ($dllPath in $RuntimeDlls) {
        Copy-Item -LiteralPath $dllPath -Destination (Join-Path $DestinationDir ([System.IO.Path]::GetFileName($dllPath))) -Force
    }
}

$repoRoot = Split-Path $PSScriptRoot -Parent
$cargoExe = Get-LlvmCargo
$mingwBin = Get-MingwBin
$llvmObjdump = Join-Path $mingwBin "llvm-objdump.exe"

if (-not (Test-Path $llvmObjdump)) {
    throw "No se encontro llvm-objdump.exe en $mingwBin"
}

$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$env:Path = (Join-Path (Split-Path $cargoExe -Parent) "") + ";" + $mingwBin + ";" + $machinePath + ";" + $userPath
$env:CARGO_TARGET_DIR = $TargetDir

if (-not $NoBuild) {
    Push-Location $repoRoot
    try {
        & $cargoExe build --release
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    }
    finally {
        Pop-Location
    }
}

$releaseDir = Join-Path $repoRoot "$TargetDir\release"
$exePath = Join-Path $releaseDir "echo-audio.exe"
if (-not (Test-Path $exePath)) {
    throw "No se encontro el binario release: $exePath"
}

$stagingDir = Join-Path $repoRoot $OutputDir
$runtimeDlls = Get-RuntimeDllPaths -BinaryPath $exePath -MingwBin $mingwBin -ObjdumpPath $llvmObjdump
Copy-ReleaseArtifacts -ExePath $exePath -RuntimeDlls $runtimeDlls -DestinationDir $releaseDir
Copy-ReleaseArtifacts -ExePath $exePath -RuntimeDlls $runtimeDlls -DestinationDir $stagingDir

Write-Host "Paquete listo en: $stagingDir"
Write-Host "Release autocontenido en: $releaseDir"
Write-Host "Archivos incluidos:"
Get-ChildItem $stagingDir -File | ForEach-Object { Write-Host " - $($_.Name)" }
