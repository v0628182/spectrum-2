#include "warzone_audio/CApi.h"
#include "warzone_audio/RealtimeEngine.h"

#include <new>

struct WzaRealtimeEngine {
    warzone_audio::RealtimeEngine engine;
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
    out.protectionExtreme = params.protectionExtreme != 0;
    out.debugLogging = params.debugLogging != 0;
    return out;
}

} // namespace

extern "C" {

WZA_EXPORT WzaRealtimeEngine* wza_rt_create_engine(void)
{
    try {
        auto* engine = new WzaRealtimeEngine();
        if (!engine->engine.prepare(2048, 8)) {
            delete engine;
            return nullptr;
        }
        return engine;
    } catch (...) {
        return nullptr;
    }
}

WZA_EXPORT void wza_rt_destroy_engine(WzaRealtimeEngine* engine)
{
    delete engine;
}

WZA_EXPORT int wza_rt_prepare_engine(WzaRealtimeEngine* engine, size_t maxFrames, size_t maxChannels)
{
    if (!engine) {
        return 0;
    }
    return engine->engine.prepare(maxFrames, maxChannels) ? 1 : 0;
}

WZA_EXPORT void wza_rt_reset_engine(WzaRealtimeEngine* engine)
{
    if (engine) {
        engine->engine.reset();
    }
}

WZA_EXPORT void wza_rt_set_params(WzaRealtimeEngine* engine, const WzaEngineParams* params)
{
    if (engine && params) {
        engine->engine.setParams(toCppParams(*params));
    }
}

WZA_EXPORT void wza_rt_process_interleaved(WzaRealtimeEngine* engine,
                                           const float* input,
                                           float* output,
                                           size_t frames,
                                           size_t channels)
{
    if (engine) {
        engine->engine.processInterleaved(input, output, frames, channels);
    }
}

WZA_EXPORT void wza_rt_get_scores(WzaRealtimeEngine* engine, WzaScores* scores)
{
    if (!engine || !scores) {
        return;
    }

    const auto snapshot = engine->engine.snapshot();
    scores->footstep = snapshot.footstep;
    scores->action = snapshot.action;
    scores->protection = snapshot.protection;
    scores->lateral = snapshot.lateral;
    scores->confidence = snapshot.confidence;
    scores->outputPeak = snapshot.outputPeak;
    scores->framesAnalyzed = snapshot.framesAnalyzed;
}

} // extern "C"
