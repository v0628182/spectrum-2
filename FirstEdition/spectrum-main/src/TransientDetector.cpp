#include "TransientDetector.h"

#include <algorithm>
#include <cmath>

#include "MathUtils.h"

namespace warzone_audio {

void TransientDetector::reset()
{
    footstepState_ = 0.0f;
    actionState_ = 0.0f;
    protectionState_ = 0.0f;
}

DetectorScores TransientDetector::update(const FeatureFrame& f, const EngineParams& params)
{
    const float sensitivity = clamp(params.detectionSensitivity / 100.0f, 0.0f, 1.0f);
    const float thresholdScale = 1.20f + (0.75f - 1.20f) * sensitivity;

    const float sFlux = ramp(f.superFluxStepExcess, constants::kSuperFluxStepMin * thresholdScale,
                             constants::kSuperFluxStepStrong * thresholdScale);
    const float sSnr = ramp(f.snrDb.step, 6.0f * thresholdScale, 18.0f * thresholdScale);
    const float sAttack = ramp(f.attackStepDb, constants::kStepAttackMinDb * thresholdScale,
                               constants::kStepAttackStrongDb * thresholdScale);
    const float sCentroid = ramp(f.centroidHz, constants::kCentroidStepMinHz,
                                 constants::kCentroidStepIdealHz);
    const float sFlatness = saturate(1.0f - std::abs(f.flatnessStep - constants::kFlatnessStepTarget) /
                                              constants::kFlatnessStepTarget);
    const float sDuration =
        (f.durationMs >= constants::kStepMinDurationMs && f.durationMs <= constants::kStepMaxDurationMs) ? 1.0f : 0.0f;
    const float sStepDominance = ramp(f.energyDb.step - f.energyDb.lowMid, -3.0f, 6.0f);
    const float broadbandPenalty = 1.0f - 0.75f * ramp(static_cast<float>(f.activeBands), 4.0f, 5.0f);
    const float sQuietEvent = 1.0f - ramp(f.inputPeak, 0.075f, 0.220f);

    const float sBass = ramp(f.snrDb.bass, 12.0f, 30.0f);
    const float sLowMid = ramp(f.snrDb.lowMid, 10.0f, 28.0f);
    const float sTotal = ramp(f.snrDb.total, 14.0f, 32.0f);
    const float sBroadband = ramp(static_cast<float>(f.activeBands), 3.0f, 5.0f);
    const float sCrest = ramp(f.crestDb, constants::kCrestProtectionMinDb, 18.0f);
    const float sPeak = ramp(f.inputPeak, 0.16f, 0.42f);
    const float sImpactBody = ramp(std::max(f.energyDb.lowMid, f.energyDb.bass), 10.0f, 24.0f);

    float protection = saturate(0.22f * sBass + 0.18f * sLowMid + 0.22f * sTotal +
                                0.16f * sBroadband + 0.08f * sCrest +
                                0.08f * sPeak + 0.06f * sImpactBody);

    const float footstepEvidence = saturate(0.25f * sFlux + 0.22f * sSnr + 0.20f * sAttack +
                                            0.10f * sCentroid + 0.08f * sFlatness +
                                            0.10f * sStepDominance + 0.05f * sDuration);
    const float sActionSnrForStep = ramp(std::max(f.snrDb.mid, f.snrDb.step), 5.0f * thresholdScale,
                                         16.0f * thresholdScale);
    const float sSoftCrest = ramp(f.crestDb, 5.5f, 9.5f);
    const float sSoftFlatness = ramp(f.flatnessStep, 0.18f, 0.48f) * (1.0f - 0.45f * ramp(f.flatnessStep, 0.78f, 1.0f));
    const float softFootstepEvidence =
        saturate(0.34f * sSnr + 0.24f * sActionSnrForStep + 0.16f * sSoftCrest +
                 0.14f * sSoftFlatness + 0.08f * sDuration + 0.04f * sStepDominance) *
        sQuietEvent * (1.0f - 0.55f * sBroadband);
    const float notBroadband = 1.0f - sBroadband;
    const float combinedFootstepEvidence = std::max(footstepEvidence, softFootstepEvidence);
    protection *= 1.0f - 0.65f * combinedFootstepEvidence * notBroadband;
    protection = std::max(protection, 0.75f * sPeak * sImpactBody);

    const float sProtectionPenalty = 1.0f - std::max(protection, protectionState_);
    float footstep = sProtectionPenalty *
                     broadbandPenalty *
                     combinedFootstepEvidence;

    if (sFlux < 0.15f || sAttack < 0.12f || sSnr < 0.10f) {
        footstep *= 0.35f + 0.65f * softFootstepEvidence;
    }

    const float sActionFlux = ramp(f.superFluxPresenceExcess + f.superFluxStepExcess, 0.05f, 0.18f);
    const float sActionSnr = ramp(std::max(f.snrDb.mid, f.snrDb.step), 5.0f, 16.0f);
    const float sActionDuration = ramp(f.durationMs, 10.0f, 160.0f) * (1.0f - ramp(f.durationMs, 160.0f, 300.0f));
    float action = (1.0f - protection) *
                   (0.45f * sActionFlux + 0.35f * sActionSnr + 0.20f * sActionDuration);

    footstepState_ = approachDb(footstepState_, footstep, 3.0f, 90.0f);
    actionState_ = approachDb(actionState_, action, 5.0f, 100.0f);
    protectionState_ = approachDb(protectionState_, protection, 1.0f, 140.0f);

    DetectorScores scores;
    scores.footstep = saturate(footstepState_);
    scores.action = saturate(actionState_);
    scores.protection = saturate(protectionState_);
    scores.lateral = f.lateral;
    scores.confidence = std::max({scores.footstep, scores.action, scores.protection});
    scores.impact = saturate(sPeak * sImpactBody);
    return scores;
}

} // namespace warzone_audio
