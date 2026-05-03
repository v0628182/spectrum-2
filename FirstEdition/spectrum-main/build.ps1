$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$buildDir = Join-Path $root "build"
New-Item -ItemType Directory -Force -Path $buildDir | Out-Null

$compiler = $null
foreach ($candidate in @("g++", "clang++")) {
    $cmd = Get-Command $candidate -ErrorAction SilentlyContinue
    if ($cmd) {
        $compiler = $cmd.Source
        break
    }
}

if (-not $compiler) {
    throw "No C++ compiler found. Install g++ or clang++ and retry."
}

$common = @(
    "-std=c++17",
    "-O2",
    "-Wall",
    "-Wextra",
    "-pedantic",
    "-I", (Join-Path $root "include"),
    "-I", (Join-Path $root "src")
)

$sources = @(
    (Join-Path $root "src\CApi.cpp"),
    (Join-Path $root "src\Biquad.cpp"),
    (Join-Path $root "src\Config.cpp"),
    (Join-Path $root "src\DspEngine.cpp"),
    (Join-Path $root "src\Fft.cpp"),
    (Join-Path $root "src\FeatureExtractor.cpp"),
    (Join-Path $root "src\Processor.cpp"),
    (Join-Path $root "src\RealtimeEngine.cpp"),
    (Join-Path $root "src\ScoreLogger.cpp"),
    (Join-Path $root "src\TransientDetector.cpp")
)

$dllSources = $sources
$dllOut = Join-Path $buildDir "warzone_audio_core.dll"
& $compiler @common @dllSources "-shared" "-o" $dllOut

$test = Join-Path $root "tools\synthetic_test.cpp"
$out = Join-Path $buildDir "synthetic_test.exe"

& $compiler @common @sources $test "-o" $out

$benchmark = Join-Path $root "tools\benchmark.cpp"
$benchmarkOut = Join-Path $buildDir "benchmark.exe"
& $compiler @common @sources $benchmark "-o" $benchmarkOut

$realtimeSim = Join-Path $root "tools\realtime_sim.cpp"
$realtimeSimOut = Join-Path $buildDir "realtime_sim.exe"
& $compiler @common @sources $realtimeSim "-o" $realtimeSimOut

$wavProcess = Join-Path $root "tools\wav_process.cpp"
$wavProcessOut = Join-Path $buildDir "wav_process.exe"
& $compiler @common @sources $wavProcess "-o" $wavProcessOut

$fixture = Join-Path $root "tools\generate_fixture_wav.cpp"
$fixtureOut = Join-Path $buildDir "generate_fixture_wav.exe"
& $compiler @common $fixture "-o" $fixtureOut

$wavStats = Join-Path $root "tools\wav_stats.cpp"
$wavStatsOut = Join-Path $buildDir "wav_stats.exe"
& $compiler @common $wavStats "-o" $wavStatsOut

$validateAnnotations = Join-Path $root "tools\validate_annotations.cpp"
$validateAnnotationsOut = Join-Path $buildDir "validate_annotations.exe"
& $compiler @common @sources $validateAnnotations "-o" $validateAnnotationsOut

$spectrumAnalyze = Join-Path $root "tools\spectrum_analyze.cpp"
$spectrumAnalyzeOut = Join-Path $buildDir "spectrum_analyze.exe"
& $compiler @common (Join-Path $root "src\Fft.cpp") $spectrumAnalyze "-o" $spectrumAnalyzeOut

Write-Host "Built $dllOut"
Write-Host "Built $out"
Write-Host "Built $benchmarkOut"
Write-Host "Built $realtimeSimOut"
Write-Host "Built $wavProcessOut"
Write-Host "Built $fixtureOut"
Write-Host "Built $wavStatsOut"
Write-Host "Built $validateAnnotationsOut"
Write-Host "Built $spectrumAnalyzeOut"
