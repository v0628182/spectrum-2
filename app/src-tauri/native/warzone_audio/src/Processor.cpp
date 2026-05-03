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
    const float guardAmount = clamp(params.footstepGuardAmount / 100.0f, 0.0f, 1.0f);
    const float floorDb = clamp(params.spectralFloorDb, -60.0f, -12.0f);
    const float protectionAttackMs = lerp(constants::kProtectionAttackMs, 5.0f, stability);
    const float protectionReleaseMs =
        lerp(constants::kProtectionReleaseMs, std::max(150.0f, params.stableReleaseMs), stability);
    const float boostAttackMs = lerp(constants::kBoostAttackMs, 8.0f, stability * 0.65f);
    const float boostReleaseMs = lerp(constants::kBoostReleaseMs, 160.0f, stability);
    const float maxCutStepDb = lerp(48.0f, clamp(params.maxCutStepDb, 3.0f, 24.0f), stability);
    const float maxRecoverStepDb = lerp(48.0f, std::max(2.0f, maxCutStepDb * 0.45f), stability);

    const float protection = scores.protection;
    const float confirmedFootstep = ramp(scores.footstep, 0.45f, 0.72f);
    const float rawImpactBlock = ramp(scores.impact, 0.22f, 0.70f);
    const float impactBlock = rawImpactBlock * (1.0f - lerp(0.90f, 0.98f, guardAmount) * confirmedFootstep);
    const float footstepProtect = ramp(scores.footstep, 0.38f, 0.68f);
    const float protectionForCuts = protection * (1.0f - lerp(0.65f, 0.90f, guardAmount) * footstepProtect);
    const float footstepPresence = ramp(scores.footstep, 0.32f, 0.66f);
    const float actionAsWeapon = ramp(scores.action, 0.46f, 0.76f) * (1.0f - lerp(0.85f, 0.96f, guardAmount) * footstepPresence);
    const float sustainedEnergy = std::max(actionAsWeapon, ramp(protectionForCuts, 0.18f, 0.55f));
    sustainedWeaponState_ = approachDb(sustainedWeaponState_, sustainedEnergy, 1.0f, params.sustainedHoldMs);

    const float weaponDuck = std::max(ramp(protectionForCuts, 0.32f, 0.78f), sustainedWeaponState_ * 0.85f);
    const float extremeScale = params.protectionExtreme ? 1.0f : 0.55f;

    float targetLowShelf = constants::kBassReductionDbMax * explosionAmount * protectionForCuts;
    float targetLowMid = constants::kLowMidReductionDbMax * gunAmount * protectionForCuts;
    float targetWeaponMid = params.weaponMidCutDb * gunAmount * extremeScale * weaponDuck;
    float targetStepBody = params.stepBodyBoostDb * footstepAmount * scores.footstep;
    float targetStep = params.stepClarityBoostDb * footstepAmount * scores.footstep;
    float targetAir = constants::kActionBoostDbMax * actionAmount * std::max(scores.action, scores.footstep * 0.5f);
    float targetMasterDuck = params.masterDuckDb * gunAmount * extremeScale * weaponDuck;

    const float footstepBodyPreserve = footstepAmount * scores.footstep * (1.0f - weaponDuck * lerp(0.35f, 0.12f, guardAmount));
    targetLowShelf += params.stepLowBodyBoostDb * footstepBodyPreserve;
    targetLowMid += params.stepLowMidBoostDb * footstepBodyPreserve;

    if (protectionForCuts > constants::kProtectionTrigger || weaponDuck > 0.0f) {
        const float hardProtect = std::max(protectionForCuts, weaponDuck);
        targetLowShelf += -24.0f * explosionAmount * extremeScale * hardProtect;
        targetLowMid += -24.0f * gunAmount * extremeScale * hardProtect;
        targetWeaponMid += -18.0f * gunAmount * extremeScale * hardProtect;
        targetAir = std::min(targetAir, 0.0f) + params.weaponAirCutDb * gunAmount * extremeScale * hardProtect;

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
        targetLowShelf += -36.0f * impactBlock;
        targetLowMid += -30.0f * impactBlock;
        targetWeaponMid += -24.0f * impactBlock;
        targetAir += -22.0f * impactBlock;
        if (confirmedFootstep < 0.35f) {
            targetStep = std::min(targetStep, 1.5f * footstepAmount * scores.footstep);
        }
        targetMasterDuck += params.impactDuckDb * impactBlock;
    }

    targetLowShelf = clampCutFloor(targetLowShelf, floorDb);
    targetLowMid = clampCutFloor(targetLowMid, floorDb);
    targetWeaponMid = clampCutFloor(targetWeaponMid, floorDb);
    targetAir = clampCutFloor(targetAir, floorDb);
    targetMasterDuck = clampCutFloor(targetMasterDuck, floorDb);

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
    const float levelerAllowed = levelerAmount * scores.footstep * (1.0f - std::max(scores.protection, scores.impact));
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
        channel.lowShelf.setLowShelf(constants::kSampleRate, 250.0f, 0.707f, lowShelfDb_);
        channel.lowMid.setPeaking(constants::kSampleRate, 650.0f, 0.90f, lowMidDb_);
        channel.weaponMid.setPeaking(constants::kSampleRate, 1600.0f, 0.85f, weaponMidDb_);
        channel.stepBody.setPeaking(constants::kSampleRate, 1550.0f, 1.35f, stepBodyDb_);
        channel.step.setPeaking(constants::kSampleRate, 3500.0f, 1.85f, stepDb_);
        channel.air.setPeaking(constants::kSampleRate, 6500.0f, 1.00f, airDb_);
    }
}

float Processor::limit(float x)
{
    const float absX = std::abs(x);
    if (absX > ceilingAmp_) {
        limiterGain_ = std::min(limiterGain_, ceilingAmp_ / (absX + constants::kEpsAmp));
    } else {
        const float release = std::exp(-1.0f / (constants::kSampleRate * 0.050f));
        limiterGain_ = 1.0f - release + release * limiterGain_;
    }
    return clamp(x * limiterGain_, -ceilingAmp_, ceilingAmp_);
}

void Processor::processSample(float inL, float inR, float& outL, float& outR, float& peak)
{
    const float midIn = 0.5f * (inL + inR);
    const float rmsAlpha = std::exp(-1.0f / (constants::kSampleRate * 0.035f));
    rmsState_ = rmsAlpha * rmsState_ + (1.0f - rmsAlpha) * midIn * midIn;

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

    const float duckGain = dbToAmp(masterDuckDb_ + footstepLevelerDb_);
    outL = limit(l * duckGain);
    outR = limit(r * duckGain);
    peak = std::max(peak, std::max(std::abs(outL), std::abs(outR)));
}

} // namespace warzone_audio
