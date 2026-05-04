#include "TransientDetector.h"

#include <algorithm>
#include <cmath>

#include "MathUtils.h"

namespace warzone_audio {

namespace {

float lerp(float a, float b, float t)
{
    return a + (b - a) * t;
}

} // namespace

void TransientDetector::reset()
{
    footstepState_ = 0.0f;
    actionState_ = 0.0f;
    protectionState_ = 0.0f;
}

DetectorScores TransientDetector::update(const FeatureFrame& f, const EngineParams& params)
{
    const float sensitivity = clamp(params.detectionSensitivity / 100.0f, 0.0f, 4.0f);
    const float easy = lerp(1.20f, 0.42f, saturate(sensitivity));
    const float extreme = clamp((sensitivity - 1.0f) / 3.0f, 0.0f, 1.0f);
    const float threshold = lerp(easy, 0.18f, extreme);

    const float broadbandFlux = ramp(f.superFluxBroadbandExcess, 0.045f * threshold, 0.180f * threshold);
    const float presenceFlux = ramp(f.superFluxPresenceExcess, 0.035f * threshold, 0.150f * threshold);
    const float stepFlux = ramp(f.superFluxStepExcess, 0.030f * threshold, 0.130f * threshold);

    const float bassSnr = ramp(f.snrDb.bass, 8.0f * threshold, 26.0f * threshold);
    const float lowMidSnr = ramp(f.snrDb.lowMid, 7.0f * threshold, 24.0f * threshold);
    const float midSnr = ramp(f.snrDb.mid, 7.0f * threshold, 24.0f * threshold);
    const float stepSnr = ramp(f.snrDb.step, 6.0f * threshold, 20.0f * threshold);
    const float airSnr = ramp(f.snrDb.air, 7.0f * threshold, 22.0f * threshold);
    const float totalSnr = ramp(f.snrDb.total, 10.0f * threshold, 30.0f * threshold);

    const float activeBands = ramp(static_cast<float>(f.activeBands), 3.0f, 5.0f);
    const float crest = ramp(f.crestDb, 8.0f, 17.5f);
    const float peak = ramp(f.inputPeak, 0.055f, 0.34f);
    const float shortImpact = ramp(std::max(f.attackStepDb, f.attackLowMidDb), 4.0f * threshold, 17.0f * threshold);
    const float centroidGun = ramp(f.centroidHz, 1500.0f, 6200.0f);
    const float noisyFlatness = ramp(f.flatnessStep, 0.38f, 0.92f);

    const float footstepTone = ramp(f.snrDb.step - std::max(f.snrDb.bass, f.snrDb.lowMid), -2.0f, 10.0f);
    const float footstepDuration =
        (f.durationMs >= 8.0f && f.durationMs <= 180.0f) ? 1.0f : (1.0f - ramp(f.durationMs, 180.0f, 340.0f));
    const float footstepCentroid = ramp(f.centroidHz, 1200.0f, 3600.0f) * (1.0f - ramp(f.centroidHz, 5600.0f, 9000.0f));
    const float notHugePeak = 1.0f - ramp(f.inputPeak, 0.10f, 0.30f);
    const float footstepEvidence =
        saturate(0.24f * stepFlux + 0.23f * stepSnr + 0.18f * footstepTone +
                 0.14f * footstepDuration + 0.11f * footstepCentroid +
                 0.10f * notHugePeak) *
        (1.0f - 0.72f * activeBands);

    const float weaponBody = std::sqrt(std::max(0.0f, bassSnr * lowMidSnr));
    const float weaponCrackAir = std::sqrt(std::max(0.0f, std::max(midSnr, stepSnr) * airSnr));
    const float weaponBroadband = std::min(std::max(weaponBody, midSnr), std::max(weaponCrackAir, totalSnr));
    const float impact =
        saturate(0.26f * peak + 0.22f * crest + 0.20f * shortImpact +
                 0.18f * broadbandFlux + 0.14f * activeBands);
    float gunshot =
        saturate(0.25f * impact + 0.22f * weaponBroadband + 0.20f * weaponCrackAir +
                 0.14f * presenceFlux + 0.10f * centroidGun + 0.09f * noisyFlatness);

    const float guard = std::max(saturate(params.footstepGuardAmount / 100.0f), saturate(params.protectionPasos / 100.0f));
    gunshot *= 1.0f - 0.22f * guard * footstepEvidence;

    const float tail =
        saturate(0.38f * weaponBroadband + 0.26f * totalSnr + 0.20f * airSnr + 0.16f * lowMidSnr) *
        (1.0f - 0.18f * guard * footstepEvidence);

    footstepState_ = approachDb(footstepState_, footstepEvidence, 1.5f, 95.0f);
    protectionState_ = approachDb(protectionState_, gunshot, 0.08f, clamp(params.protectionReleaseMs, 4.0f, 220.0f));
    actionState_ = approachDb(actionState_, tail, 0.35f, clamp(params.sustainedHoldMs, 20.0f, 3000.0f));

    DetectorScores scores;
    scores.footstep = saturate(footstepState_);
    scores.action = saturate(actionState_);
    scores.protection = saturate(protectionState_);
    scores.lateral = f.lateral;
    scores.impact = saturate(impact * (1.0f - 0.70f * guard * scores.footstep));
    scores.confidence = std::max({scores.footstep, scores.action, scores.protection, scores.impact});
    return scores;
}

} // namespace warzone_audio
