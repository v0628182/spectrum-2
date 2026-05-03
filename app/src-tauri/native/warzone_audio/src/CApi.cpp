#include "warzone_audio/CApi.h"

#include <algorithm>
#include <new>
#include <vector>

#include "warzone_audio/DspEngine.h"

struct WzaEngine {
    warzone_audio::DspEngine engine;
    std::vector<float> left;
    std::vector<float> right;
    std::vector<float> outLeft;
    std::vector<float> outRight;
};

namespace {

warzone_audio::EngineParams toCppParams(const WzaEngineParams& params)
{
    warzone_audio::EngineParams out;
    out.footstepEnhance = params.footstepEnhance;
    out.actionDetail = params.actionDetail;
    out.gunshotReduction = params.gunshotReduction;
    out.explosionReduction = params.explosionReduction;
    out.detectionSensitivity = params.detectionSensitivity;
    out.outputCeilingDb = params.outputCeilingDb;
    out.stepBodyBoostDb = params.stepBodyBoostDb;
    out.stepClarityBoostDb = params.stepClarityBoostDb;
    out.stepLowBodyBoostDb = params.stepLowBodyBoostDb;
    out.stepLowMidBoostDb = params.stepLowMidBoostDb;
    out.weaponMidCutDb = params.weaponMidCutDb;
    out.weaponAirCutDb = params.weaponAirCutDb;
    out.sustainedHoldMs = params.sustainedHoldMs;
    out.masterDuckDb = params.masterDuckDb;
    out.impactDuckDb = params.impactDuckDb;
    out.footstepLevelerAmount = params.footstepLevelerAmount;
    out.footstepTargetRmsDb = params.footstepTargetRmsDb;
    out.footstepMaxLiftDb = params.footstepMaxLiftDb;
    out.footstepLevelerSpeedMs = params.footstepLevelerSpeedMs;
    out.stabilityAmount = params.stabilityAmount;
    out.spectralFloorDb = params.spectralFloorDb;
    out.stableReleaseMs = params.stableReleaseMs;
    out.footstepGuardAmount = params.footstepGuardAmount;
    out.maxCutStepDb = params.maxCutStepDb;
    out.transientKill = params.transientKill;
    out.lookaheadMs = params.lookaheadMs;
    out.outputTrimDb = params.outputTrimDb;
    out.residualReductionDb = params.residualReductionDb;
    out.balanceLowDb = params.balanceLowDb;
    out.balanceMidDb = params.balanceMidDb;
    out.balanceHighDb = params.balanceHighDb;
    out.stftCutoffHz = params.stftCutoffHz;
    out.stftPreserveDb = params.stftPreserveDb;
    out.spectralFloorStab = params.spectralFloorStab;
    out.protectionPasos = params.protectionPasos;
    out.protectionExtreme = params.protectionExtreme != 0;
    out.spectralMaskEnabled = params.spectralMaskEnabled != 0;
    out.debugLogging = params.debugLogging != 0;
    return out;
}

} // namespace

WzaEngine* wza_create_engine(void)
{
    try {
        auto* engine = new WzaEngine();
        wza_prepare_engine(engine, 2048);
        return engine;
    } catch (...) {
        return nullptr;
    }
}

void wza_destroy_engine(WzaEngine* engine)
{
    delete engine;
}

void wza_reset_engine(WzaEngine* engine)
{
    if (engine) {
        engine->engine.reset();
    }
}

int wza_prepare_engine(WzaEngine* engine, size_t maxFrames)
{
    if (!engine) {
        return 0;
    }
    try {
        engine->left.assign(maxFrames, 0.0f);
        engine->right.assign(maxFrames, 0.0f);
        engine->outLeft.assign(maxFrames, 0.0f);
        engine->outRight.assign(maxFrames, 0.0f);
        return 1;
    } catch (...) {
        engine->left.clear();
        engine->right.clear();
        engine->outLeft.clear();
        engine->outRight.clear();
        return 0;
    }
}

void wza_set_params(WzaEngine* engine, const WzaEngineParams* params)
{
    if (engine && params) {
        engine->engine.setParams(toCppParams(*params));
    }
}

void wza_process_stereo(WzaEngine* engine,
                        const float* inL,
                        const float* inR,
                        float* outL,
                        float* outR,
                        size_t numSamples)
{
    if (engine && inL && inR && outL && outR) {
        engine->engine.processBlock(inL, inR, outL, outR, numSamples);
    }
}

void wza_process_interleaved(WzaEngine* engine,
                             const float* input,
                             float* output,
                             size_t frames,
                             size_t channels)
{
    if (!engine || !input || !output || frames == 0 || channels == 0) {
        return;
    }

    if (engine->left.size() < frames || engine->right.size() < frames ||
        engine->outLeft.size() < frames || engine->outRight.size() < frames) {
        std::copy(input, input + frames * channels, output);
        return;
    }

    auto& left = engine->left;
    auto& right = engine->right;
    auto& outLeft = engine->outLeft;
    auto& outRight = engine->outRight;

    for (size_t i = 0; i < frames; ++i) {
        const float* frame = input + i * channels;
        if (channels == 1) {
            left[i] = frame[0];
            right[i] = frame[0];
        } else if (channels == 2) {
            left[i] = frame[0];
            right[i] = frame[1];
        } else {
            // Common Windows order: FL, FR, FC, LFE, BL, BR, SL, SR.
            const float fl = frame[0];
            const float fr = frame[1];
            const float fc = channels > 2 ? frame[2] : 0.0f;
            const float lfe = channels > 3 ? frame[3] : 0.0f;
            const float bl = channels > 4 ? frame[4] : 0.0f;
            const float br = channels > 5 ? frame[5] : 0.0f;
            const float sl = channels > 6 ? frame[6] : 0.0f;
            const float sr = channels > 7 ? frame[7] : 0.0f;
            left[i] = fl + 0.7071f * fc + 0.25f * lfe + 0.7071f * bl + 0.7071f * sl;
            right[i] = fr + 0.7071f * fc + 0.25f * lfe + 0.7071f * br + 0.7071f * sr;
            const float peak = std::max(std::abs(left[i]), std::abs(right[i]));
            if (peak > 1.0f) {
                left[i] /= peak;
                right[i] /= peak;
            }
        }
    }

    engine->engine.processBlock(left.data(), right.data(), outLeft.data(), outRight.data(), frames);

    for (size_t i = 0; i < frames; ++i) {
        float* frame = output + i * channels;
        if (channels == 1) {
            frame[0] = 0.5f * (outLeft[i] + outRight[i]);
        } else {
            frame[0] = outLeft[i];
            frame[1] = outRight[i];
            for (size_t ch = 2; ch < channels; ++ch) {
                frame[ch] = 0.0f;
            }
        }
    }
}

void wza_get_scores(WzaEngine* engine, WzaScores* scores)
{
    if (!engine || !scores) {
        return;
    }

    const auto& stats = engine->engine.stats();
    scores->footstep = stats.scores.footstep;
    scores->action = stats.scores.action;
    scores->protection = stats.scores.protection;
    scores->lateral = stats.scores.lateral;
    scores->confidence = stats.scores.confidence;
    scores->outputPeak = stats.outputPeak;
    scores->framesAnalyzed = static_cast<unsigned long long>(stats.framesAnalyzed);
}
