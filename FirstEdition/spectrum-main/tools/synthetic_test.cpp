#include <cmath>
#include <iostream>
#include <string>
#include <vector>

#include "warzone_audio/Config.h"
#include "warzone_audio/DspEngine.h"
#include "warzone_audio/ScoreLogger.h"

namespace {

float noiseSample(unsigned& state)
{
    state = 1664525u * state + 1013904223u;
    const float unit = static_cast<float>((state >> 8) & 0x00FFFFFFu) / static_cast<float>(0x00FFFFFFu);
    return 2.0f * unit - 1.0f;
}

void addSineBurst(std::vector<float>& l,
                  std::vector<float>& r,
                  float startSec,
                  float durationSec,
                  float freqHz,
                  float amp,
                  float pan)
{
    const std::size_t start = static_cast<std::size_t>(startSec * warzone_audio::constants::kSampleRate);
    const std::size_t count = static_cast<std::size_t>(durationSec * warzone_audio::constants::kSampleRate);
    const float leftGain = std::sqrt(0.5f * (1.0f - pan));
    const float rightGain = std::sqrt(0.5f * (1.0f + pan));

    for (std::size_t i = 0; i < count && start + i < l.size(); ++i) {
        const float t = static_cast<float>(i) / warzone_audio::constants::kSampleRate;
        const float env = std::sin(warzone_audio::constants::kPi * static_cast<float>(i) / static_cast<float>(count));
        const float s = amp * env * std::sin(2.0f * warzone_audio::constants::kPi * freqHz * t);
        l[start + i] += s * leftGain;
        r[start + i] += s * rightGain;
    }
}

void addExplosion(std::vector<float>& l, std::vector<float>& r, float startSec, float durationSec, float amp)
{
    unsigned rng = 0x12345678u;
    const std::size_t start = static_cast<std::size_t>(startSec * warzone_audio::constants::kSampleRate);
    const std::size_t count = static_cast<std::size_t>(durationSec * warzone_audio::constants::kSampleRate);

    float low = 0.0f;
    for (std::size_t i = 0; i < count && start + i < l.size(); ++i) {
        const float env = std::exp(-5.0f * static_cast<float>(i) / static_cast<float>(count));
        low = 0.985f * low + 0.015f * noiseSample(rng);
        const float broadband = noiseSample(rng) * 0.35f;
        const float s = amp * env * (low + broadband);
        l[start + i] += s;
        r[start + i] += s;
    }
}

struct ScenarioResult {
    std::string name;
    float maxFootstep = 0.0f;
    float maxAction = 0.0f;
    float maxProtection = 0.0f;
    float outputPeak = 0.0f;
    std::size_t framesAnalyzed = 0;
};

ScenarioResult runScenario(const std::string& name,
                           const std::vector<float>& inL,
                           const std::vector<float>& inR,
                           const warzone_audio::AppConfig& config,
                           bool writeLog)
{
    std::vector<float> outL(inL.size(), 0.0f);
    std::vector<float> outR(inR.size(), 0.0f);

    warzone_audio::DspEngine engine;
    engine.setParams(config.engine);

    warzone_audio::ScoreLogger logger;
    if (writeLog && config.engine.debugLogging) {
        logger.open(config.logging.logPath, config.logging.logEveryFrames);
    }

    ScenarioResult result;
    result.name = name;

    constexpr std::size_t blockSize = 64;
    for (std::size_t offset = 0; offset < inL.size(); offset += blockSize) {
        const std::size_t count = std::min(blockSize, inL.size() - offset);
        engine.processBlock(inL.data() + offset, inR.data() + offset, outL.data() + offset, outR.data() + offset, count);
        const auto& stats = engine.stats();
        result.maxFootstep = std::max(result.maxFootstep, stats.scores.footstep);
        result.maxAction = std::max(result.maxAction, stats.scores.action);
        result.maxProtection = std::max(result.maxProtection, stats.scores.protection);
        logger.write(stats);
    }

    for (std::size_t i = 0; i < outL.size(); ++i) {
        result.outputPeak = std::max(result.outputPeak, std::max(std::abs(outL[i]), std::abs(outR[i])));
    }
    result.framesAnalyzed = engine.stats().framesAnalyzed;
    return result;
}

void addAmbience(std::vector<float>& l, std::vector<float>& r, float amp)
{
    unsigned rng = 0xC0FFEEu;
    for (std::size_t i = 0; i < l.size(); ++i) {
        const float ambience = amp * noiseSample(rng);
        l[i] += ambience;
        r[i] += ambience;
    }
}

} // namespace

int main()
{
    constexpr std::size_t totalSamples = static_cast<std::size_t>(warzone_audio::constants::kSampleRate * 2.0f);
    std::vector<float> inL(totalSamples, 0.0f);
    std::vector<float> inR(totalSamples, 0.0f);
    warzone_audio::AppConfig config;
    std::string configError;
    if (!warzone_audio::loadConfigFile("config/default_settings.ini", config, &configError)) {
        std::cerr << "config_load_warning=" << configError << "\n";
    }
    config.engine.footstepEnhance = 75.0f;
    config.engine.gunshotReduction = 95.0f;
    config.engine.explosionReduction = 100.0f;
    config.engine.detectionSensitivity = 70.0f;
    config.engine.debugLogging = true;

    addAmbience(inL, inR, 0.01f);
    addSineBurst(inL, inR, 0.45f, 0.025f, 3500.0f, 0.16f, 0.35f);
    addExplosion(inL, inR, 1.05f, 0.180f, 0.95f);
    const auto mixed = runScenario("mixed_step_explosion", inL, inR, config, true);

    std::vector<float> ambienceL(totalSamples, 0.0f);
    std::vector<float> ambienceR(totalSamples, 0.0f);
    addAmbience(ambienceL, ambienceR, 0.012f);
    const auto ambience = runScenario("ambience_only", ambienceL, ambienceR, config, false);

    std::vector<float> stepL(totalSamples, 0.0f);
    std::vector<float> stepR(totalSamples, 0.0f);
    addAmbience(stepL, stepR, 0.008f);
    addSineBurst(stepL, stepR, 0.50f, 0.022f, 3500.0f, 0.18f, -0.45f);
    addSineBurst(stepL, stepR, 0.82f, 0.020f, 4100.0f, 0.14f, 0.30f);
    const auto steps = runScenario("steps_only", stepL, stepR, config, false);

    std::vector<float> explosionL(totalSamples, 0.0f);
    std::vector<float> explosionR(totalSamples, 0.0f);
    addAmbience(explosionL, explosionR, 0.006f);
    addExplosion(explosionL, explosionR, 0.70f, 0.200f, 1.1f);
    const auto explosion = runScenario("explosion_only", explosionL, explosionR, config, false);

    const ScenarioResult results[] = {mixed, ambience, steps, explosion};
    for (const auto& r : results) {
        std::cout << r.name
                  << " frames=" << r.framesAnalyzed
                  << " footstep=" << r.maxFootstep
                  << " action=" << r.maxAction
                  << " protection=" << r.maxProtection
                  << " peak=" << r.outputPeak << "\n";
    }

    const bool ok = mixed.maxFootstep > 0.25f &&
                    mixed.maxProtection > 0.35f &&
                    steps.maxFootstep > ambience.maxFootstep + 0.10f &&
                    explosion.maxProtection > 0.35f &&
                    ambience.maxProtection < 0.20f &&
                    mixed.outputPeak <= 0.900f &&
                    steps.outputPeak <= 0.900f &&
                    explosion.outputPeak <= 0.900f;
    std::cout << (ok ? "PASS" : "FAIL") << "\n";
    return ok ? 0 : 1;
}
