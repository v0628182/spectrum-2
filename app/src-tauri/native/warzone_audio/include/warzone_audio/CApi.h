#pragma once

#include <stddef.h>

#ifdef _WIN32
#define WZA_EXPORT __declspec(dllexport)
#else
#define WZA_EXPORT
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct WzaEngine WzaEngine;

typedef struct WzaEngineParams {
    float footstepEnhance;
    float actionDetail;
    float gunshotReduction;
    float explosionReduction;
    float detectionSensitivity;
    float outputCeilingDb;
    float stepBodyBoostDb;
    float stepClarityBoostDb;
    float stepLowBodyBoostDb;
    float stepLowMidBoostDb;
    float weaponMidCutDb;
    float weaponAirCutDb;
    float sustainedHoldMs;
    float masterDuckDb;
    float impactDuckDb;
    float footstepLevelerAmount;
    float footstepTargetRmsDb;
    float footstepMaxLiftDb;
    float footstepLevelerSpeedMs;
    float stabilityAmount;
    float spectralFloorDb;
    float stableReleaseMs;
    float footstepGuardAmount;
    float maxCutStepDb;
    float transientKill;
    float lookaheadMs;
    float outputTrimDb;
    float residualReductionDb;
    float balanceLowDb;
    float balanceMidDb;
    float balanceHighDb;
    float stftCutoffHz;
    float stftPreserveDb;
    float spectralFloorStab;
    float protectionPasos;
    float weaponOnlyMode;
    float changeIntensity;
    float subtletyAmount;
    float wetMix;
    float lowShelfFreqHz;
    float lowMidFreqHz;
    float lowMidQ;
    float weaponMidFreqHz;
    float weaponMidQ;
    float stepBodyFreqHz;
    float stepBodyQ;
    float stepClarityFreqHz;
    float stepClarityQ;
    float weaponAirFreqHz;
    float weaponAirQ;
    float protectionAttackMs;
    float protectionReleaseMs;
    float boostAttackMs;
    float boostReleaseMs;
    float limiterReleaseMs;
    float stereoWidth;
    float weaponMuteAmount;
    float weaponSilencerAmount;
    float silencerBodyAmount;
    float silencerCrackAmount;
    float silencerAirAmount;
    float silencerTailAmount;
    float silencerSideAmount;
    float silencerRestoreAmount;
    int protectionExtreme;
    int spectralMaskEnabled;
    int debugLogging;
} WzaEngineParams;

typedef struct WzaScores {
    float footstep;
    float action;
    float protection;
    float lateral;
    float confidence;
    float outputPeak;
    unsigned long long framesAnalyzed;
} WzaScores;

WZA_EXPORT WzaEngine* wza_create_engine(void);
WZA_EXPORT void wza_destroy_engine(WzaEngine* engine);
WZA_EXPORT void wza_reset_engine(WzaEngine* engine);
WZA_EXPORT int wza_prepare_engine(WzaEngine* engine, size_t maxFrames);
WZA_EXPORT void wza_set_params(WzaEngine* engine, const WzaEngineParams* params);
WZA_EXPORT void wza_process_stereo(WzaEngine* engine,
                                   const float* inL,
                                   const float* inR,
                                   float* outL,
                                   float* outR,
                                   size_t numSamples);
WZA_EXPORT void wza_process_interleaved(WzaEngine* engine,
                                        const float* input,
                                        float* output,
                                        size_t frames,
                                        size_t channels);
WZA_EXPORT void wza_get_scores(WzaEngine* engine, WzaScores* scores);

#ifdef __cplusplus
}
#endif
