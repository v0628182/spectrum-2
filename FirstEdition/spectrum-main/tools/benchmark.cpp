#include <chrono>
#include <cmath>
#include <iostream>
#include <vector>

#include "warzone_audio/DspEngine.h"

namespace {

float noiseSample(unsigned& state)
{
    state = 1664525u * state + 1013904223u;
    const float unit = static_cast<float>((state >> 8) & 0x00FFFFFFu) / static_cast<float>(0x00FFFFFFu);
    return 2.0f * unit - 1.0f;
}

} // namespace

int main()
{
    constexpr std::size_t seconds = 30;
    constexpr std::size_t blockSize = 128;
    constexpr std::size_t totalSamples = static_cast<std::size_t>(warzone_audio::constants::kSampleRate) * seconds;

    std::vector<float> inL(totalSamples, 0.0f);
    std::vector<float> inR(totalSamples, 0.0f);
    std::vector<float> outL(totalSamples, 0.0f);
    std::vector<float> outR(totalSamples, 0.0f);

    unsigned rng = 0xBEEFu;
    for (std::size_t i = 0; i < totalSamples; ++i) {
        const float t = static_cast<float>(i) / warzone_audio::constants::kSampleRate;
        const float tone = 0.02f * std::sin(2.0f * warzone_audio::constants::kPi * 3500.0f * t);
        const float noise = 0.01f * noiseSample(rng);
        inL[i] = tone + noise;
        inR[i] = 0.8f * tone + noise;
    }

    warzone_audio::DspEngine engine;
    warzone_audio::EngineParams params;
    params.footstepEnhance = 75.0f;
    params.explosionReduction = 90.0f;
    params.gunshotReduction = 90.0f;
    params.detectionSensitivity = 60.0f;
    engine.setParams(params);

    const auto start = std::chrono::high_resolution_clock::now();
    for (std::size_t offset = 0; offset < totalSamples; offset += blockSize) {
        const std::size_t count = std::min(blockSize, totalSamples - offset);
        engine.processBlock(inL.data() + offset, inR.data() + offset, outL.data() + offset, outR.data() + offset, count);
    }
    const auto end = std::chrono::high_resolution_clock::now();

    const double elapsedMs = std::chrono::duration<double, std::milli>(end - start).count();
    const double realtimeMs = 1000.0 * static_cast<double>(seconds);
    const double realtimeFactor = realtimeMs / elapsedMs;
    const double cpuPercentOneCore = 100.0 / realtimeFactor;

    std::cout << "processed_seconds=" << seconds << "\n";
    std::cout << "elapsed_ms=" << elapsedMs << "\n";
    std::cout << "realtime_factor=" << realtimeFactor << "\n";
    std::cout << "single_core_cpu_percent_estimate=" << cpuPercentOneCore << "\n";
    std::cout << "frames_analyzed=" << engine.stats().framesAnalyzed << "\n";
    return realtimeFactor > 10.0 ? 0 : 1;
}
