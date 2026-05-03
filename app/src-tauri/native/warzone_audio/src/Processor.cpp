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

float clampCutFloor(float valueDb, float floorDb)
{
    return valueDb < 0.0f ? std::max(valueDb, floorDb) : valueDb;
}

float scaleBySign(float valueDb, float cutScale, float boostScale)
{
    return valueDb < 0.0f ? valueDb * cutScale : valueDb * boostScale;
}

} // namespace

void Processor::reset()
{
    for (auto& channel : channels_) {
        channel.lowShelf.reset();
        channel.lowMid.reset();
        channel.weaponMid.reset();
        channel.stepBody.reset();
        channel.step.reset();
        channel.air.reset();
    }
    lowShelfDb_ = 0.0f;
    lowMidDb_ = 0.0f;
    weaponMidDb_ = 0.0f;
    stepBodyDb_ = 0.0f;
    stepDb_ = 0.0f;
    airDb_ = 0.0f;
    masterDuckDb_ = 0.0f;
    outputTrimDb_ = 0.0f;
    maskCutoffHz_ = 2500.0f;
    lowShelfFreqHz_ = 250.0f;
    lowMidFreqHz_ = 650.0f;
    lowMidQ_ = 0.90f;
    weaponMidFreqHz_ = 1600.0f;
    weaponMidQ_ = 0.85f;
    stepBodyFreqHz_ = 1550.0f;
    stepBodyQ_ = 1.35f;
    stepClarityFreqHz_ = 3500.0f;
    stepClarityQ_ = 1.85f;
    weaponAirFreqHz_ = 6500.0f;
    weaponAirQ_ = 1.00f;
    wetMix_ = 1.0f;
    limiterReleaseMs_ = 0.5f;
    stereoWidth_ = 1.0f;
    weaponOnlyMode_ = false;
    sustainedWeaponState_ = 0.0f;
    footstepLevelerDb_ = 0.0f;
    rmsState_ = 0.0f;
    limiterGain_ = 1.0f;
    updateFilters();
}

void Processor::updateTargets(const DetectorScores& scores, const EngineParams& params)
{
    const float footstepAmount = clamp(params.footstepEnhance / 100.0f, 0.0f, 1.0f);
    const float actionAmount = clamp(params.actionDetail / 100.0f, 0.0f, 1.0f);
    const float gunAmount = clamp(params.gunshotReduction / 100.0f, 0.0f, 1.0f);
    const float explosionAmount = clamp(params.explosionReduction / 100.0f, 0.0f, 1.0f);
    const float stability = clamp(params.stabilityAmount / 100.0f, 0.0f, 1.0f);
    const float intensity = clamp(params.changeIntensity / 100.0f, 0.0f, 2.0f);
    const float subtlety = clamp(params.subtletyAmount / 100.0f, 0.0f, 1.0f);
    const float cutScale = intensity * lerp(1.35f, 0.42f, subtlety);
    const float boostScale = intensity * lerp(1.20f, 0.38f, subtlety);
    const float guardAmount =
        std::max(clamp(params.footstepGuardAmount / 100.0f, 0.0f, 1.0f),
                 clamp(params.protectionPasos / 100.0f, 0.0f, 1.0f));
    const float floorDb = clamp(lerp(params.spectralFloorDb, params.spectralFloorStab, stability), -60.0f, -12.0f);
    const float lookaheadAssist = clamp(params.lookaheadMs / 2.0f, 0.0f, 1.0f);
    const float baseProtectionAttackMs = lerp(params.protectionAttackMs, 0.1f, lookaheadAssist);
    float protectionAttackMs =
        clamp(lerp(baseProtectionAttackMs, baseProtectionAttackMs * 1.85f, subtlety * 0.45f), 0.1f, 90.0f);
    float protectionReleaseMs =
        clamp(lerp(params.protectionReleaseMs, std::max(150.0f, params.stableReleaseMs), stability * 0.35f), 5.0f, 900.0f);
    float boostAttackMs = clamp(lerp(params.boostAttackMs, params.boostAttackMs * 1.70f, subtlety * 0.35f), 0.1f, 90.0f);
    float boostReleaseMs = clamp(lerp(params.boostReleaseMs, 260.0f, subtlety * 0.50f), 5.0f, 900.0f);
    if (params.weaponOnlyMode >= 0.5f) {
        protectionAttackMs = clamp(params.protectionAttackMs, 0.1f, 12.0f);
        protectionReleaseMs = clamp(params.protectionReleaseMs, 0.5f, 80.0f);
        boostAttackMs = clamp(params.boostAttackMs, 0.1f, 12.0f);
        boostReleaseMs = clamp(params.boostReleaseMs, 0.5f, 80.0f);
    }
    const float maxCutStepDb = lerp(48.0f, clamp(params.maxCutStepDb, 3.0f, 24.0f), stability);
    const float maxRecoverStepDb = lerp(48.0f, std::max(2.0f, maxCutStepDb * 0.45f), stability);

    const bool weaponOnlyMode = params.weaponOnlyMode >= 0.5f;
    const float protection = scores.protection;
    const float confirmedFootstep = ramp(scores.footstep, 0.45f, 0.72f);
    const float transientAmount = clamp(params.transientKill / 100.0f, 0.0f, 1.0f);
    const float maskAmount = params.spectralMaskEnabled ? 1.0f : 0.0f;
    const float rawImpactBlock = ramp(scores.impact, 0.22f, 0.70f) * transientAmount;
    const float impactBlock = rawImpactBlock * (1.0f - lerp(0.90f, 0.98f, guardAmount) * confirmedFootstep);
    const float footstepProtect = ramp(scores.footstep, 0.38f, 0.68f);
    const float protectionForCuts =
        protection * maskAmount * (1.0f - lerp(0.65f, 0.90f, guardAmount) * footstepProtect);
    const float footstepPresence = ramp(scores.footstep, 0.32f, 0.66f);
    const float actionAsWeapon = ramp(scores.action, 0.46f, 0.76f) * (1.0f - lerp(0.85f, 0.96f, guardAmount) * footstepPresence);
    const float sustainedEnergy = std::max(actionAsWeapon, ramp(protectionForCuts, 0.18f, 0.55f));
    const float sustainedReleaseMs = weaponOnlyMode ? clamp(params.sustainedHoldMs, 0.5f, 120.0f) : params.sustainedHoldMs;
    sustainedWeaponState_ = approachDb(sustainedWeaponState_, sustainedEnergy, 0.25f, sustainedReleaseMs);

    const float instantWeaponDuck = std::max(
        std::max(ramp(protectionForCuts, 0.32f, 0.78f), rawImpactBlock),
        ramp(scores.action, 0.55f, 0.86f));
    const float weaponDuck =
        weaponOnlyMode ? instantWeaponDuck : std::max(ramp(protectionForCuts, 0.32f, 0.78f), sustainedWeaponState_ * 0.85f);
    const float footstepWeaponOverlap =
        clamp(std::max(weaponDuck, rawImpactBlock) * footstepPresence * guardAmount, 0.0f, 1.0f);
    const float extremeScale = params.protectionExtreme ? 1.0f : 0.55f;
    wetMix_ = clamp(params.wetMix / 100.0f, 0.0f, 1.0f);
    limiterReleaseMs_ = clamp(params.limiterReleaseMs, 0.5f, 250.0f);
    stereoWidth_ = clamp(params.stereoWidth / 100.0f, 0.50f, 1.60f);
    weaponOnlyMode_ = weaponOnlyMode;

    const float residualCut = clamp(params.residualReductionDb, -24.0f, 0.0f) * maskAmount;
    float targetLowShelf = constants::kBassReductionDbMax * explosionAmount * protectionForCuts;
    float targetLowMid = constants::kLowMidReductionDbMax * gunAmount * protectionForCuts;
    float targetWeaponMid = params.weaponMidCutDb * gunAmount * extremeScale * weaponDuck;
    float targetStepBody = params.stepBodyBoostDb * footstepAmount * scores.footstep;
    float targetStep = (params.stepClarityBoostDb + params.stftPreserveDb) * footstepAmount * scores.footstep;
    float targetAir = constants::kActionBoostDbMax * actionAmount * std::max(scores.action, scores.footstep * 0.5f);
    float targetMasterDuck = 0.0f;
    maskCutoffHz_ = clamp(params.stftCutoffHz, 500.0f, 8000.0f);

    const float footstepBodyPreserve = footstepAmount * scores.footstep * (1.0f - weaponDuck * lerp(0.35f, 0.12f, guardAmount));
    targetLowShelf += params.stepLowBodyBoostDb * footstepBodyPreserve;
    targetLowMid += params.stepLowMidBoostDb * footstepBodyPreserve;

    const float footstepEscape = clamp(1.0f - lerp(0.82f, 1.0f, guardAmount) * footstepPresence, 0.0f, 1.0f);
    const float weaponDepthDb = -std::min(params.masterDuckDb, 0.0f) * gunAmount * extremeScale * weaponDuck;
    targetLowMid += -weaponDepthDb * 0.28f * footstepEscape;
    targetWeaponMid += -weaponDepthDb * 0.95f;
    targetAir += -weaponDepthDb * 0.72f * footstepEscape;

    if (protectionForCuts > constants::kProtectionTrigger || weaponDuck > 0.0f) {
        const float hardProtect = std::max(protectionForCuts, weaponDuck);
        const float lowBandProtectScale = lerp(1.0f, 0.0f, footstepWeaponOverlap);
        targetLowShelf += -24.0f * explosionAmount * extremeScale * hardProtect * lowBandProtectScale;
        targetLowMid += -24.0f * gunAmount * extremeScale * hardProtect * lowBandProtectScale;
        targetWeaponMid += -18.0f * gunAmount * extremeScale * hardProtect;
        targetAir = std::min(targetAir, 0.0f) + params.weaponAirCutDb * gunAmount * extremeScale * hardProtect * footstepEscape;
        targetLowMid += residualCut * hardProtect * 0.65f * lowBandProtectScale;
        targetWeaponMid += residualCut * hardProtect;
        targetAir += residualCut * hardProtect * 0.85f * footstepEscape;

        // Keep the core footstep band alive; gunshots are broadband, but footsteps need 2.5-5 kHz.
        const float protectedStepBoost =
            params.stepClarityBoostDb * footstepAmount * std::max(scores.footstep, scores.action * 0.45f);
        const float protectedBodyBoost = params.stepBodyBoostDb * footstepAmount * scores.footstep;
        targetWeaponMid *= 1.0f - lerp(0.65f, 0.92f, guardAmount) * scores.footstep;
        targetStepBody = std::max(targetStepBody, protectedBodyBoost);
        targetStep = std::max(targetStep, protectedStepBoost);
        targetAir += 0.55f * params.stepBodyBoostDb * footstepAmount * scores.footstep * (1.0f - impactBlock);
    }

    if (impactBlock > 0.0f) {
        const float impactFootstepEscape = clamp(1.0f - lerp(0.75f, 0.96f, guardAmount) * confirmedFootstep, 0.04f, 1.0f);
        const float impactDepthDb = -std::min(params.impactDuckDb, 0.0f) * impactBlock;
        targetLowShelf += -36.0f * impactBlock * impactFootstepEscape;
        targetLowMid += -30.0f * impactBlock * impactFootstepEscape;
        targetWeaponMid += -24.0f * impactBlock;
        targetAir += -22.0f * impactBlock * impactFootstepEscape;
        targetWeaponMid += -impactDepthDb * 0.90f;
        targetAir += -impactDepthDb * 0.62f * impactFootstepEscape;
        targetLowMid += -impactDepthDb * 0.18f * impactFootstepEscape;
        targetWeaponMid += residualCut * impactBlock;
        targetAir += residualCut * impactBlock * 0.75f * impactFootstepEscape;
        if (confirmedFootstep < 0.35f) {
            targetStep = std::min(targetStep, 1.5f * footstepAmount * scores.footstep);
        }
    }

    if (weaponOnlyMode) {
        const float weaponOnlyStrength =
            clamp(std::max(std::max(weaponDuck, rawImpactBlock), ramp(scores.action, 0.50f, 0.82f)), 0.0f, 1.0f);
        const float weaponDepthDb = -std::min(params.masterDuckDb, 0.0f);
        const float impactDepthDb = -std::min(params.impactDuckDb, 0.0f) * rawImpactBlock;
        targetLowShelf = 0.0f;
        targetLowMid = 0.0f;
        targetStepBody = 0.0f;
        targetStep = 0.0f;
        targetMasterDuck = 0.0f;
        targetWeaponMid =
            (params.weaponMidCutDb * gunAmount * extremeScale - weaponDepthDb * 0.90f + residualCut) * weaponOnlyStrength;
        targetAir =
            (params.weaponAirCutDb * gunAmount * extremeScale - impactDepthDb * 0.45f + residualCut * 0.45f) *
            weaponOnlyStrength;
    }

    targetLowShelf = clampCutFloor(targetLowShelf, floorDb);
    targetLowMid = clampCutFloor(targetLowMid, floorDb);
    targetWeaponMid = clampCutFloor(targetWeaponMid, floorDb);
    targetAir = clampCutFloor(targetAir, floorDb);
    targetMasterDuck = clampCutFloor(targetMasterDuck, floorDb);

    targetLowShelf = scaleBySign(targetLowShelf, cutScale, boostScale);
    targetLowMid = scaleBySign(targetLowMid, cutScale, boostScale);
    targetWeaponMid = scaleBySign(targetWeaponMid, cutScale, boostScale);
    targetStepBody = scaleBySign(targetStepBody, cutScale, boostScale);
    targetStep = scaleBySign(targetStep, cutScale, boostScale);
    targetAir = scaleBySign(targetAir, cutScale, boostScale);
    targetMasterDuck = scaleBySign(targetMasterDuck, cutScale, boostScale);

    if (!weaponOnlyMode && footstepWeaponOverlap > 0.0f) {
        const float lowFloor = lerp(floorDb, -1.0f, footstepWeaponOverlap);
        const float lowMidFloor = lerp(floorDb, -1.5f, footstepWeaponOverlap);
        const float weaponMidFloor = lerp(floorDb, -9.0f, footstepWeaponOverlap);
        const float airFloor = lerp(floorDb, -6.0f, footstepWeaponOverlap);
        targetLowShelf = std::max(targetLowShelf, lowFloor);
        targetLowMid = std::max(targetLowMid, lowMidFloor);
        targetWeaponMid = std::max(targetWeaponMid, weaponMidFloor);
        targetAir = std::max(targetAir, airFloor);
        targetMasterDuck = 0.0f;

        const float protectedBodyBoost = params.stepBodyBoostDb * footstepAmount * scores.footstep;
        const float protectedStepBoost =
            (params.stepClarityBoostDb + params.stftPreserveDb) * footstepAmount * scores.footstep;
        targetStepBody = std::max(targetStepBody, protectedBodyBoost);
        targetStep = std::max(targetStep, protectedStepBoost);
    }

    targetLowShelf += clamp(params.balanceLowDb, -12.0f, 12.0f);
    targetLowMid += clamp(params.balanceMidDb, -12.0f, 12.0f) * 0.65f;
    targetStepBody += clamp(params.balanceMidDb, -12.0f, 12.0f) * 0.35f;
    targetAir += clamp(params.balanceHighDb, -12.0f, 12.0f);
    outputTrimDb_ = clamp(params.outputTrimDb, -20.0f, 6.0f);
    lowShelfFreqHz_ = clamp(params.lowShelfFreqHz, 80.0f, 500.0f);
    lowMidFreqHz_ = clamp(params.lowMidFreqHz, 250.0f, 1200.0f);
    lowMidQ_ = clamp(params.lowMidQ, 0.25f, 3.0f);
    const float weaponOnlyFilterAmount = weaponOnlyMode ? 1.0f : footstepWeaponOverlap;
    const float baseWeaponMidFreqHz = clamp(params.weaponMidFreqHz, 700.0f, 3600.0f);
    const float baseWeaponMidQ = clamp(params.weaponMidQ, 0.25f, 4.0f);
    weaponMidFreqHz_ = lerp(baseWeaponMidFreqHz, std::max(baseWeaponMidFreqHz, 2400.0f), weaponOnlyFilterAmount);
    weaponMidQ_ = lerp(baseWeaponMidQ, std::max(baseWeaponMidQ, 2.60f), weaponOnlyFilterAmount);
    stepBodyFreqHz_ = clamp(params.stepBodyFreqHz, 600.0f, 2600.0f);
    stepBodyQ_ = clamp(params.stepBodyQ, 0.25f, 5.0f);
    stepClarityFreqHz_ = clamp(params.stepClarityFreqHz, 1800.0f, 6200.0f);
    stepClarityQ_ = clamp(params.stepClarityQ, 0.25f, 6.0f);
    const float baseWeaponAirFreqHz = clamp(params.weaponAirFreqHz, 3000.0f, 12000.0f);
    const float baseWeaponAirQ = clamp(params.weaponAirQ, 0.25f, 5.0f);
    weaponAirFreqHz_ = lerp(baseWeaponAirFreqHz, std::max(baseWeaponAirFreqHz, 7200.0f), weaponOnlyFilterAmount);
    weaponAirQ_ = lerp(baseWeaponAirQ, std::max(baseWeaponAirQ, 2.40f), weaponOnlyFilterAmount);

    lowShelfDb_ = slewControl(
        lowShelfDb_, approachDb(lowShelfDb_, targetLowShelf, protectionAttackMs, protectionReleaseMs),
        maxCutStepDb, maxRecoverStepDb);
    lowMidDb_ = slewControl(
        lowMidDb_, approachDb(lowMidDb_, targetLowMid, protectionAttackMs, protectionReleaseMs),
        maxCutStepDb, maxRecoverStepDb);
    weaponMidDb_ = slewControl(
        weaponMidDb_, approachDb(weaponMidDb_, targetWeaponMid, protectionAttackMs, protectionReleaseMs),
        maxCutStepDb, maxRecoverStepDb);
    stepBodyDb_ = approachDb(stepBodyDb_, targetStepBody, boostAttackMs, boostReleaseMs);
    stepDb_ = approachDb(stepDb_, targetStep, boostAttackMs, boostReleaseMs);
    airDb_ = slewControl(
        airDb_, approachDb(airDb_, targetAir, boostAttackMs, boostReleaseMs),
        maxCutStepDb, maxRecoverStepDb);
    masterDuckDb_ = slewControl(
        masterDuckDb_, approachDb(masterDuckDb_, targetMasterDuck, lerp(0.5f, 4.0f, stability), lerp(70.0f, 170.0f, stability)),
        maxCutStepDb, maxRecoverStepDb);

    const float levelerAmount = clamp(params.footstepLevelerAmount / 100.0f, 0.0f, 1.0f);
    const float levelerAllowed =
        weaponOnlyMode ? 0.0f : levelerAmount * scores.footstep * (1.0f - std::max(scores.protection, scores.impact));
    const float currentRmsDb = ampToDb(std::sqrt(rmsState_ + constants::kEpsEnergy));
    const float neededLift = clamp(params.footstepTargetRmsDb - currentRmsDb, 0.0f, params.footstepMaxLiftDb);
    const float targetLevelerDb = neededLift * levelerAllowed;
    footstepLevelerDb_ = approachDb(footstepLevelerDb_, targetLevelerDb, params.footstepLevelerSpeedMs, 120.0f);
    ceilingAmp_ = dbToAmp(params.outputCeilingDb);

    updateFilters();
}

float Processor::slewControl(float current, float target, float maxDownDb, float maxUpDb) const
{
    const float delta = target - current;
    const float lo = -std::abs(maxDownDb);
    const float hi = std::abs(maxUpDb);
    return current + clamp(delta, lo, hi);
}

void Processor::updateFilters()
{
    for (auto& channel : channels_) {
        channel.lowShelf.setLowShelf(constants::kSampleRate, lowShelfFreqHz_, 0.707f, lowShelfDb_);
        channel.lowMid.setPeaking(constants::kSampleRate, lowMidFreqHz_, lowMidQ_, lowMidDb_);
        channel.weaponMid.setPeaking(
            constants::kSampleRate,
            std::max(weaponMidFreqHz_, maskCutoffHz_ * 0.64f),
            weaponMidQ_,
            weaponMidDb_);
        channel.stepBody.setPeaking(constants::kSampleRate, stepBodyFreqHz_, stepBodyQ_, stepBodyDb_);
        channel.step.setPeaking(constants::kSampleRate, stepClarityFreqHz_, stepClarityQ_, stepDb_);
        channel.air.setPeaking(
            constants::kSampleRate,
            std::max(weaponAirFreqHz_, maskCutoffHz_ * 1.55f),
            weaponAirQ_,
            airDb_);
    }
}

float Processor::limit(float x)
{
    const float absX = std::abs(x);
    if (absX > ceilingAmp_) {
        limiterGain_ = std::min(limiterGain_, ceilingAmp_ / (absX + constants::kEpsAmp));
    } else {
        const float releaseSeconds = limiterReleaseMs_ * 0.001f;
        const float release = std::exp(-1.0f / (constants::kSampleRate * releaseSeconds));
        limiterGain_ = 1.0f - release + release * limiterGain_;
    }
    return clamp(x * limiterGain_, -ceilingAmp_, ceilingAmp_);
}

void Processor::processSample(float inL, float inR, float& outL, float& outR, float& peak)
{
    const float midIn = 0.5f * (inL + inR);
    const float rmsAlpha = std::exp(-1.0f / (constants::kSampleRate * 0.035f));
    rmsState_ = rmsAlpha * rmsState_ + (1.0f - rmsAlpha) * midIn * midIn;

    if (weaponOnlyMode_) {
        const float sideIn = 0.5f * (inL - inR);
        float weaponMid = channels_[0].weaponMid.process(midIn);
        weaponMid = channels_[0].air.process(weaponMid);
        const float mixedMid = midIn + (weaponMid - midIn) * wetMix_;
        outL = clamp(mixedMid + sideIn, -ceilingAmp_, ceilingAmp_);
        outR = clamp(mixedMid - sideIn, -ceilingAmp_, ceilingAmp_);
        peak = std::max(peak, std::max(std::abs(outL), std::abs(outR)));
        return;
    }

    float l = channels_[0].lowShelf.process(inL);
    l = channels_[0].lowMid.process(l);
    l = channels_[0].weaponMid.process(l);
    l = channels_[0].stepBody.process(l);
    l = channels_[0].step.process(l);
    l = channels_[0].air.process(l);

    float r = channels_[1].lowShelf.process(inR);
    r = channels_[1].lowMid.process(r);
    r = channels_[1].weaponMid.process(r);
    r = channels_[1].stepBody.process(r);
    r = channels_[1].step.process(r);
    r = channels_[1].air.process(r);

    const float duckGain = dbToAmp(masterDuckDb_ + footstepLevelerDb_ + outputTrimDb_);
    const float wetL = limit(l * duckGain);
    const float wetR = limit(r * duckGain);
    float mixedL = inL + (wetL - inL) * wetMix_;
    float mixedR = inR + (wetR - inR) * wetMix_;

    if (std::abs(stereoWidth_ - 1.0f) > 0.001f) {
        const float mid = 0.5f * (mixedL + mixedR);
        const float side = 0.5f * (mixedL - mixedR) * stereoWidth_;
        mixedL = mid + side;
        mixedR = mid - side;
    }

    outL = clamp(mixedL, -ceilingAmp_, ceilingAmp_);
    outR = clamp(mixedR, -ceilingAmp_, ceilingAmp_);
    peak = std::max(peak, std::max(std::abs(outL), std::abs(outR)));
}

} // namespace warzone_audio
