#pragma once

#include <array>
#include <cstddef>

#include "warzone_audio/Constants.h"

namespace warzone_audio {

struct EngineParams {
    float footstepEnhance = 65.0f;
    float actionDetail = 45.0f;
    float gunshotReduction = 85.0f;
    float explosionReduction = 90.0f;
    float detectionSensitivity = 55.0f;
    float outputCeilingDb = constants::kOutputCeilingDb;
    float stepBodyBoostDb = 11.0f;
    float stepClarityBoostDb = constants::kFootstepBoostDbMax;
    float stepLowBodyBoostDb = 8.0f;
    float stepLowMidBoostDb = 7.0f;
    float weaponMidCutDb = -30.0f;
    float weaponAirCutDb = -28.0f;
    float sustainedHoldMs = 0.5f;
    float masterDuckDb = -10.0f;
    float impactDuckDb = -24.0f;
    float footstepLevelerAmount = 0.0f;
    float footstepTargetRmsDb = -24.0f;
    float footstepMaxLiftDb = 10.0f;
    float footstepLevelerSpeedMs = 80.0f;
    float stabilityAmount = 0.0f;
    float spectralFloorDb = -42.0f;
    float stableReleaseMs = 0.5f;
    float footstepGuardAmount = 70.0f;
    float maxCutStepDb = 48.0f;
    float transientKill = 70.0f;
    float lookaheadMs = 0.0f;
    float outputTrimDb = 0.0f;
    float residualReductionDb = 0.0f;
    float balanceLowDb = 0.0f;
    float balanceMidDb = 0.0f;
    float balanceHighDb = 0.0f;
    float stftCutoffHz = 2500.0f;
    float stftPreserveDb = 0.0f;
    float spectralFloorStab = -34.0f;
    float protectionPasos = 85.0f;
    float weaponOnlyMode = 0.0f;
    float changeIntensity = 100.0f;
    float subtletyAmount = 35.0f;
    float wetMix = 100.0f;
    float lowShelfFreqHz = 250.0f;
    float lowMidFreqHz = 650.0f;
    float lowMidQ = 0.90f;
    float weaponMidFreqHz = 1600.0f;
    float weaponMidQ = 0.85f;
    float stepBodyFreqHz = 1550.0f;
    float stepBodyQ = 1.35f;
    float stepClarityFreqHz = 3500.0f;
    float stepClarityQ = 1.85f;
    float weaponAirFreqHz = 6500.0f;
    float weaponAirQ = 1.00f;
    float protectionAttackMs = 0.5f;
    float protectionReleaseMs = 0.5f;
    float boostAttackMs = 0.5f;
    float boostReleaseMs = 0.5f;
    float limiterReleaseMs = 0.5f;
    float stereoWidth = 100.0f;
    bool protectionExtreme = true;
    bool spectralMaskEnabled = true;
    bool debugLogging = false;
};

struct BandEnergiesDb {
    float bass = -120.0f;
    float lowMid = -120.0f;
    float mid = -120.0f;
    float step = -120.0f;
    float air = -120.0f;
    float noise = -120.0f;
    float total = -120.0f;
};

struct FeatureFrame {
    BandEnergiesDb energyDb;
    BandEnergiesDb noiseDb;
    BandEnergiesDb snrDb;

    float superFluxStep = 0.0f;
    float superFluxPresence = 0.0f;
    float superFluxBroadband = 0.0f;
    float superFluxStepExcess = 0.0f;
    float superFluxPresenceExcess = 0.0f;
    float superFluxBroadbandExcess = 0.0f;
    float centroidHz = 0.0f;
    float flatnessStep = 0.0f;
    float crestDb = 0.0f;
    float inputPeak = 0.0f;
    float attackStepDb = 0.0f;
    float attackLowMidDb = 0.0f;
    float durationMs = 0.0f;
    float lateral = 0.0f;
    int activeBands = 0;
};

struct DetectorScores {
    float footstep = 0.0f;
    float action = 0.0f;
    float protection = 0.0f;
    float lateral = 0.0f;
    float confidence = 0.0f;
    float impact = 0.0f;
};

struct ProcessStats {
    DetectorScores scores;
    FeatureFrame features;
    float outputPeak = 0.0f;
    std::size_t framesAnalyzed = 0;
};

} // namespace warzone_audio
