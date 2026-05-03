#include "warzone_audio/Config.h"
#include "warzone_audio/Constants.h"
#include "warzone_audio/RealtimeEngine.h"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <iostream>
#include <string>
#include <vector>

namespace {

constexpr float kPi = warzone_audio::constants::kPi;
constexpr float kSampleRate = warzone_audio::constants::kSampleRate;

float fract(float x)
{
    return x - std::floor(x);
}

float noise(std::size_t n)
{
    return 2.0f * fract(std::sin(static_cast<float>(n) * 12.9898f) * 43758.5453f) - 1.0f;
}

float burstEnvelope(float t, float start, float attackMs, float decayMs)
{
    const float x = t - start;
    if (x < 0.0f) {
        return 0.0f;
    }
    const float attack = attackMs * 0.001f;
    const float decay = decayMs * 0.001f;
    if (x < attack) {
        return x / attack;
    }
    return std::exp(-(x - attack) / decay);
}

float generatedSample(std::size_t n, int channel)
{
    const float t = static_cast<float>(n) / kSampleRate;
    float x = 0.008f * std::sin(2.0f * kPi * 95.0f * t);
    x += 0.006f * std::sin(2.0f * kPi * 640.0f * t + 0.45f * channel);
    x += 0.004f * noise(n + static_cast<std::size_t>(channel) * 901);

    const float step1 = burstEnvelope(t, 0.72f, 4.0f, 42.0f);
    const float step2 = burstEnvelope(t, 1.18f, 3.0f, 38.0f);
    const float step3 = burstEnvelope(t, 2.76f, 4.0f, 46.0f);
    const float step = step1 + step2 + step3;
    const float pan = channel == 0 ? 0.76f : 1.0f;
    x += pan * step * (0.060f * std::sin(2.0f * kPi * 1550.0f * t) +
                       0.050f * std::sin(2.0f * kPi * 3500.0f * t));

    const float gun = burstEnvelope(t, 1.82f, 1.5f, 170.0f);
    x += gun * (0.22f * noise(n * 3 + 17) +
                0.20f * std::sin(2.0f * kPi * 175.0f * t) +
                0.13f * std::sin(2.0f * kPi * 1600.0f * t));

    return std::max(-0.95f, std::min(0.95f, x));
}

bool isFiniteBuffer(const std::vector<float>& values)
{
    for (float v : values) {
        if (!std::isfinite(v)) {
            return false;
        }
    }
    return true;
}

} // namespace

int main(int argc, char** argv)
{
    const std::string configPath = argc > 1 ? argv[1] : "config/warzone_reference_v1.ini";

    warzone_audio::AppConfig config;
    std::string error;
    if (!warzone_audio::loadConfigFile(configPath, config, &error)) {
        std::cerr << "Could not load config: " << error << "\n";
        return 1;
    }

    warzone_audio::RealtimeEngine engine;
    if (!engine.prepare(512, 8)) {
        std::cerr << "Realtime prepare failed\n";
        return 1;
    }
    engine.setParams(config.engine);

    const std::size_t totalFrames = static_cast<std::size_t>(kSampleRate * 4.0f);
    const std::size_t blockSizes[] = {64, 128, 256, 512};
    std::vector<float> input(512 * 8, 0.0f);
    std::vector<float> output(512 * 8, 0.0f);
    float maxPeak = 0.0f;
    bool finite = true;
    bool changedA = false;
    bool changedB = false;
    bool changedC = false;

    const auto started = std::chrono::high_resolution_clock::now();

    std::size_t frame = 0;
    std::size_t blockIndex = 0;
    while (frame < totalFrames) {
        const std::size_t frames = std::min(blockSizes[blockIndex % 4], totalFrames - frame);
        const std::size_t channels = 2;

        if (!changedA && frame >= static_cast<std::size_t>(kSampleRate * 1.0f)) {
            auto params = engine.paramsSnapshot();
            params.gunshotReduction = 95.0f;
            params.sustainedHoldMs = 1200.0f;
            engine.setParams(params);
            changedA = true;
        }
        if (!changedB && frame >= static_cast<std::size_t>(kSampleRate * 2.0f)) {
            auto params = engine.paramsSnapshot();
            params.footstepEnhance = 100.0f;
            params.footstepLevelerAmount = 100.0f;
            engine.setParams(params);
            changedB = true;
        }
        if (!changedC && frame >= static_cast<std::size_t>(kSampleRate * 3.0f)) {
            auto params = engine.paramsSnapshot();
            params.stabilityAmount = 85.0f;
            params.maxCutStepDb = 6.0f;
            engine.setParams(params);
            changedC = true;
        }

        for (std::size_t i = 0; i < frames; ++i) {
            input[i * channels] = generatedSample(frame + i, 0);
            input[i * channels + 1] = generatedSample(frame + i, 1);
        }

        engine.processInterleaved(input.data(), output.data(), frames, channels);
        finite = finite && isFiniteBuffer(output);
        for (std::size_t i = 0; i < frames * channels; ++i) {
            maxPeak = std::max(maxPeak, std::abs(output[i]));
        }

        frame += frames;
        ++blockIndex;
    }

    // Quick compatibility pass for mono and 7.1-style buffers.
    std::fill(input.begin(), input.end(), 0.01f);
    engine.processInterleaved(input.data(), output.data(), 64, 1);
    engine.processInterleaved(input.data(), output.data(), 64, 8);

    const auto ended = std::chrono::high_resolution_clock::now();
    const auto elapsedMs = std::chrono::duration<double, std::milli>(ended - started).count();
    const auto snapshot = engine.snapshot();

    std::cout << "blocksProcessed=" << snapshot.blocksProcessed << "\n";
    std::cout << "parameterUpdatesApplied=" << snapshot.parameterUpdatesApplied << "\n";
    std::cout << "framesAnalyzed=" << snapshot.framesAnalyzed << "\n";
    std::cout << "maxPeak=" << maxPeak << "\n";
    std::cout << "elapsedMs=" << elapsedMs << "\n";
    std::cout << "realtimeFactor=" << (4000.0 / elapsedMs) << "\n";
    std::cout << "oversizedBlocks=" << snapshot.oversizedBlocks << "\n";
    std::cout << "bypassedBlocks=" << snapshot.bypassedBlocks << "\n";

    if (!finite) {
        std::cerr << "FAIL: non-finite output\n";
        return 1;
    }
    if (snapshot.oversizedBlocks != 0 || snapshot.bypassedBlocks != 0) {
        std::cerr << "FAIL: unexpected realtime bypass\n";
        return 1;
    }
    if (snapshot.parameterUpdatesApplied < 4) {
        std::cerr << "FAIL: parameter updates were not applied on the audio path\n";
        return 1;
    }
    if (maxPeak > 0.30f) {
        std::cerr << "FAIL: output exceeded conservative realtime peak bound\n";
        return 1;
    }

    std::cout << "PASS\n";
    return 0;
}
