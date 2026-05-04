#include "Processor.h"

#include <algorithm>
#include <cmath>

#include "MathUtils.h"

namespace warzone_audio {

namespace {

float lerp(float a, float b, float t)
{
    return a + (b - a) * t;
}

float normAmount(float value)
{
    return clamp(value / 100.0f, 0.0f, 4.0f);
}

float amountOrTotal(float amount, float total)
{
    const float normalized = normAmount(amount);
    return normalized > 0.001f ? normalized : total;
}

float componentDrive(float value)
{
    const float normal = saturate(value);
    const float over = clamp((value - 1.0f) / 3.0f, 0.0f, 1.0f);
    return normal + 1.85f * over;
}

} // namespace

void Processor::resetFilters()
{
    mid_.body.reset();
    mid_.crack.reset();
    mid_.air.reset();
    mid_.stepBody.reset();
    mid_.stepClarity.reset();
    side_.body.reset();
    side_.crack.reset();
    side_.air.reset();
    side_.stepBody.reset();
    side_.stepClarity.reset();
}

void Processor::reset()
{
    resetFilters();

    weaponOnlyMode_ = false;
    filtersDirty_ = true;

    bodyFreqHz_ = 900.0f;
    bodyQ_ = 1.15f;
    crackFreqHz_ = 2400.0f;
    crackQ_ = 1.35f;
    airFreqHz_ = 7600.0f;
    airQ_ = 1.15f;
    stepBodyFreqHz_ = 1550.0f;
    stepBodyQ_ = 1.35f;
    stepClarityFreqHz_ = 3500.0f;
    stepClarityQ_ = 1.85f;

    wetMix_ = 1.0f;
    outputTrimDb_ = 0.0f;
    ceilingAmp_ = dbToAmp(constants::kOutputCeilingDb);
    stereoWidth_ = 1.0f;

    detectorGunMask_ = 0.0f;
    detectorProtectMask_ = 0.0f;
    suppressionAmount_ = 0.0f;
    bodyAmount_ = 0.0f;
    crackAmount_ = 0.0f;
    airAmount_ = 0.0f;
    tailAmount_ = 0.0f;
    sideAmount_ = 0.0f;
    restoreAmount_ = 1.0f;
    transientAmount_ = 0.0f;
    sustainReleaseMs_ = 90.0f;
    detectorAttackMs_ = 0.05f;
    detectorReleaseMs_ = 35.0f;
    guardAmount_ = 0.85f;

    rmsState_ = 0.0f;
    dcState_ = 0.0f;
    bodyEnv_ = 0.0f;
    crackEnv_ = 0.0f;
    airEnv_ = 0.0f;
    stepEnv_ = 0.0f;
    sideEnv_ = 0.0f;
    transientEnv_ = 0.0f;
    gunHold_ = 0.0f;
    tailStateMid_ = 0.0f;
    tailStateSide_ = 0.0f;

    updateFilters();
}

void Processor::setBandTarget(float& current, float next, float tolerance)
{
    if (std::abs(current - next) > tolerance) {
        current = next;
        filtersDirty_ = true;
    }
}

void Processor::updateFilters()
{
    if (!filtersDirty_) {
        return;
    }

    const float sr = constants::kSampleRate;
    mid_.body.setBandPass(sr, clamp(bodyFreqHz_, 120.0f, 3200.0f), clamp(bodyQ_, 0.25f, 12.0f));
    mid_.crack.setBandPass(sr, clamp(crackFreqHz_, 500.0f, 8000.0f), clamp(crackQ_, 0.25f, 16.0f));
    mid_.air.setBandPass(sr, clamp(airFreqHz_, 2500.0f, 18000.0f), clamp(airQ_, 0.25f, 20.0f));
    mid_.stepBody.setBandPass(sr, clamp(stepBodyFreqHz_, 120.0f, 8000.0f), clamp(stepBodyQ_, 0.25f, 12.0f));
    mid_.stepClarity.setBandPass(sr, clamp(stepClarityFreqHz_, 300.0f, 16000.0f), clamp(stepClarityQ_, 0.25f, 16.0f));

    side_.body.setBandPass(sr, clamp(bodyFreqHz_, 120.0f, 3200.0f), clamp(bodyQ_, 0.25f, 12.0f));
    side_.crack.setBandPass(sr, clamp(crackFreqHz_, 500.0f, 8000.0f), clamp(crackQ_, 0.25f, 16.0f));
    side_.air.setBandPass(sr, clamp(airFreqHz_, 2500.0f, 18000.0f), clamp(airQ_, 0.25f, 20.0f));
    side_.stepBody.setBandPass(sr, clamp(stepBodyFreqHz_, 120.0f, 8000.0f), clamp(stepBodyQ_, 0.25f, 12.0f));
    side_.stepClarity.setBandPass(sr, clamp(stepClarityFreqHz_, 300.0f, 16000.0f), clamp(stepClarityQ_, 0.25f, 16.0f));

    filtersDirty_ = false;
}

float Processor::follow(float current, float target, float attackMs, float releaseMs) const
{
    const float tauMs = target > current ? attackMs : releaseMs;
    const float alpha = std::exp(-1.0f / (constants::kSampleRate * std::max(tauMs, 0.001f) * 0.001f));
    return alpha * current + (1.0f - alpha) * target;
}

float Processor::limit(float x) const
{
    return clamp(x, -ceilingAmp_, ceilingAmp_);
}

void Processor::updateTargets(const DetectorScores& scores, const EngineParams& params)
{
    weaponOnlyMode_ = params.weaponOnlyMode >= 0.5f;
    wetMix_ = clamp(params.wetMix / 100.0f, 0.0f, 1.0f);
    outputTrimDb_ = weaponOnlyMode_ ? 0.0f : clamp(params.outputTrimDb, -60.0f, 24.0f);
    ceilingAmp_ = dbToAmp(clamp(params.outputCeilingDb, -60.0f, -0.1f));
    stereoWidth_ = weaponOnlyMode_ ? 1.0f : clamp(params.stereoWidth / 100.0f, 0.0f, 3.0f);

    const float intensity = normAmount(params.changeIntensity);
    const float subtlety = saturate(params.subtletyAmount / 100.0f);
    const float oldMute = normAmount(params.weaponMuteAmount);
    const float totalSilencer = std::max({normAmount(params.gunshotReduction), oldMute, normAmount(params.weaponSilencerAmount)}) *
                                lerp(1.20f, 0.62f, subtlety) * std::max(0.35f, intensity);

    suppressionAmount_ = weaponOnlyMode_ ? clamp(totalSilencer, 0.0f, 4.0f) : 0.0f;
    bodyAmount_ = componentDrive(amountOrTotal(params.silencerBodyAmount, suppressionAmount_) * suppressionAmount_);
    crackAmount_ = componentDrive(amountOrTotal(params.silencerCrackAmount, suppressionAmount_) * suppressionAmount_);
    airAmount_ = componentDrive(amountOrTotal(params.silencerAirAmount, suppressionAmount_) * suppressionAmount_);
    tailAmount_ = componentDrive(amountOrTotal(params.silencerTailAmount, suppressionAmount_) * suppressionAmount_);
    sideAmount_ = componentDrive(amountOrTotal(params.silencerSideAmount, suppressionAmount_) * suppressionAmount_);
    restoreAmount_ = clamp(params.silencerRestoreAmount / 100.0f, 0.0f, 4.0f);
    transientAmount_ = componentDrive(normAmount(params.transientKill) * std::max(0.4f, suppressionAmount_));
    guardAmount_ = std::max(saturate(params.footstepGuardAmount / 100.0f), saturate(params.protectionPasos / 100.0f));

    detectorAttackMs_ = clamp(params.protectionAttackMs, 0.01f, 50.0f);
    detectorReleaseMs_ = clamp(params.protectionReleaseMs, 0.01f, 240.0f);
    sustainReleaseMs_ = clamp(params.sustainedHoldMs, 8.0f, 3000.0f);

    const float detectorGun =
        weaponOnlyMode_ ? saturate(std::max({scores.impact, scores.protection * 0.88f, scores.action * 0.76f}) *
                                   (1.0f - 0.18f * guardAmount_ * scores.footstep))
                        : 0.0f;
    const float detectorProtect = saturate(scores.footstep * guardAmount_);
    detectorGunMask_ = follow(detectorGunMask_, detectorGun, detectorAttackMs_, detectorReleaseMs_);
    detectorProtectMask_ = follow(detectorProtectMask_, detectorProtect, 0.20f, 70.0f);

    setBandTarget(bodyFreqHz_, clamp(params.lowMidFreqHz, 120.0f, 3200.0f), 8.0f);
    setBandTarget(bodyQ_, clamp(params.lowMidQ, 0.25f, 12.0f), 0.03f);
    setBandTarget(crackFreqHz_, clamp(params.weaponMidFreqHz, 500.0f, 8000.0f), 12.0f);
    setBandTarget(crackQ_, clamp(params.weaponMidQ, 0.25f, 16.0f), 0.03f);
    setBandTarget(airFreqHz_, clamp(params.weaponAirFreqHz, 2500.0f, 18000.0f), 20.0f);
    setBandTarget(airQ_, clamp(params.weaponAirQ, 0.25f, 20.0f), 0.03f);
    setBandTarget(stepBodyFreqHz_, clamp(params.stepBodyFreqHz, 120.0f, 8000.0f), 8.0f);
    setBandTarget(stepBodyQ_, clamp(params.stepBodyQ, 0.25f, 12.0f), 0.03f);
    setBandTarget(stepClarityFreqHz_, clamp(params.stepClarityFreqHz, 300.0f, 16000.0f), 12.0f);
    setBandTarget(stepClarityQ_, clamp(params.stepClarityQ, 0.25f, 16.0f), 0.03f);
    updateFilters();
}

void Processor::processSample(float inL, float inR, float& outL, float& outR, float& peak)
{
    updateFilters();

    float mid = 0.5f * (inL + inR);
    float side = 0.5f * (inL - inR);
    if (!weaponOnlyMode_ && std::abs(stereoWidth_ - 1.0f) > 0.001f) {
        side *= stereoWidth_;
    }

    const float body = mid_.body.process(mid);
    const float crack = mid_.crack.process(mid);
    const float air = mid_.air.process(mid);
    const float stepBody = mid_.stepBody.process(mid);
    const float stepClarity = mid_.stepClarity.process(mid);
    const float sideBody = side_.body.process(side);
    const float sideCrack = side_.crack.process(side);
    const float sideAir = side_.air.process(side);

    rmsState_ = follow(rmsState_, mid * mid, 0.20f, 220.0f);
    const float adaptive = std::max(0.00008f, std::sqrt(rmsState_ + constants::kEpsEnergy) * 0.42f);

    const float dcAlpha = std::exp(-1.0f / (constants::kSampleRate * 0.0017f));
    dcState_ = dcAlpha * dcState_ + (1.0f - dcAlpha) * mid;
    const float transient = mid - dcState_;

    bodyEnv_ = follow(bodyEnv_, std::abs(body), 0.025f, 34.0f);
    crackEnv_ = follow(crackEnv_, std::abs(crack), 0.015f, 20.0f);
    airEnv_ = follow(airEnv_, std::abs(air), 0.015f, 16.0f);
    stepEnv_ = follow(stepEnv_, 0.52f * std::abs(stepBody) + 0.48f * std::abs(stepClarity), 0.35f, 85.0f);
    sideEnv_ = follow(sideEnv_, std::abs(side), 0.20f, 55.0f);
    transientEnv_ = follow(transientEnv_, std::abs(transient), 0.010f, 10.0f);

    const float bodySig = ramp(bodyEnv_, adaptive * 0.75f, adaptive * 3.80f);
    const float crackSig = ramp(crackEnv_, adaptive * 0.70f, adaptive * 4.20f);
    const float airSig = ramp(airEnv_, adaptive * 0.52f, adaptive * 3.80f);
    const float transientSig = ramp(transientEnv_, adaptive * 1.15f, adaptive * 7.20f);
    const float stepSig = ramp(stepEnv_, adaptive * 0.72f, adaptive * 3.20f);
    const float sideSig = ramp(sideEnv_, adaptive * 0.90f, adaptive * 4.00f);

    const float broadbandSignature = std::min(std::max(bodySig, crackSig * 0.75f), std::max(crackSig, airSig));
    const float crackAirSignature = std::sqrt(std::max(0.0f, crackSig * airSig));
    const float centerBias = saturate(1.0f - 0.18f * sideSig);
    const float sampleWeapon =
        saturate((0.36f * transientSig + 0.28f * crackAirSignature + 0.20f * broadbandSignature +
                  0.16f * std::max(crackSig, airSig)) *
                 centerBias);
    const float sampleProtect =
        saturate(std::max(detectorProtectMask_ * 0.78f, stepSig * (0.35f + 0.65f * sideSig)) * guardAmount_ *
                 (1.0f - 0.72f * transientSig * crackAirSignature));

    float weaponMask = std::max(detectorGunMask_, sampleWeapon);
    weaponMask *= saturate(suppressionAmount_ * 0.72f);
    weaponMask = saturate(weaponMask);

    gunHold_ = follow(gunHold_, weaponMask, 0.010f, sustainReleaseMs_);
    const float activeMask = saturate(std::max(weaponMask, gunHold_ * saturate(tailAmount_ * 0.42f)));
    const float surgicalMask = saturate(activeMask * (0.32f + 0.68f * std::max(transientSig, crackAirSignature)));

    const float tailAlpha = std::exp(-1.0f / (constants::kSampleRate * 0.011f));
    tailStateMid_ = tailAlpha * tailStateMid_ + (1.0f - tailAlpha) * (0.50f * body + 0.35f * crack + 0.15f * air);
    tailStateSide_ = tailAlpha * tailStateSide_ + (1.0f - tailAlpha) * (0.50f * sideBody + 0.35f * sideCrack + 0.15f * sideAir);

    float processedMid = mid;
    float processedSide = side;

    if (activeMask > 0.0001f) {
        const float bodyCancel = clamp(bodyAmount_ * 0.55f, 0.0f, 1.70f);
        const float crackCancel = clamp(crackAmount_ * 0.82f, 0.0f, 3.20f);
        const float airCancel = clamp(airAmount_ * 0.78f, 0.0f, 3.00f);
        const float transientCancel = clamp(transientAmount_ * 0.68f, 0.0f, 3.00f);
        const float tailCancel = clamp(tailAmount_ * 0.70f, 0.0f, 2.40f);

        const float weaponEstimate =
            body * bodyCancel + crack * crackCancel + air * airCancel +
            transient * transientCancel + tailStateMid_ * tailCancel;
        processedMid = mid - weaponEstimate * surgicalMask;
        processedSide = side;

        const float nukeMask =
            saturate(activeMask * (0.45f * transientSig + 0.35f * crackAirSignature + 0.20f * broadbandSignature) *
                     saturate(suppressionAmount_ * 0.40f));
        const float overdrive = clamp((suppressionAmount_ - 1.0f) / 3.0f, 0.0f, 1.0f);
        const float directCutDb = lerp(-42.0f, -132.0f, overdrive) * nukeMask;
        const float silencedResidue =
            body * dbToAmp(lerp(-20.0f, -44.0f, saturate(bodyAmount_))) * activeMask *
            (0.10f + 0.08f * saturate(restoreAmount_));
        const float stepRestore =
            (0.34f * stepBody + 0.46f * stepClarity) * sampleProtect * saturate(restoreAmount_) *
            (1.0f - 0.48f * nukeMask);
        const float directSuppressed = processedMid * dbToAmp(directCutDb) + silencedResidue + stepRestore;
        processedMid = lerp(processedMid, directSuppressed, nukeMask);

        const float nativeRestore = saturate(sampleProtect * restoreAmount_ * 0.24f);
        processedMid = lerp(processedMid, mid, nativeRestore);
    }

    const float mixedMid = lerp(mid, processedMid, wetMix_);
    const float mixedSide = lerp(side, processedSide, wetMix_);
    const float gain = dbToAmp(outputTrimDb_);
    outL = limit((mixedMid + mixedSide) * gain);
    outR = limit((mixedMid - mixedSide) * gain);
    peak = std::max(peak, std::max(std::abs(outL), std::abs(outR)));
}

} // namespace warzone_audio
