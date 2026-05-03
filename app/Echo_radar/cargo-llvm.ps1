param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs
)

$rustBins = @(
    "C:\Program Files\Rust stable LLVM 1.94\bin"
)

$cargoExe = $null
foreach ($bin in $rustBins) {
    $candidate = Join-Path $bin "cargo.exe"
    if (Test-Path $candidate) {
        $cargoExe = $candidate
        break
    }
}

if (-not $cargoExe) {
    throw "No se encontro cargo.exe del toolchain LLVM."
}

$mingwBin = ([Environment]::GetEnvironmentVariable("Path", "User") -split ";") |
    Where-Object { $_ -like "*LLVM-MinGW*bin" } |
    Select-Object -First 1

if (-not $mingwBin) {
    throw "No se encontro LLVM-MinGW en el PATH del usuario."
}

$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$env:Path = (Join-Path (Split-Path $cargoExe -Parent) "") + ";" + $mingwBin + ";" + $machinePath + ";" + $userPath
$env:CARGO_TARGET_DIR = "target-gnullvm"

if (-not $CargoArgs -or $CargoArgs.Count -eq 0) {
    $CargoArgs = @("build")
}

& $cargoExe @CargoArgs
exit $LASTEXITCODE
