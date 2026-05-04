#include "SelfWeaponSuppressor.h"

#include <algorithm>
#include <cmath>

#include "MathUtils.h"

namespace warzone_audio {

namespace {

float lerp(float a, float b, float t)
{
    return a + (b - a) * t;
}

float signedSoftClip(float x)
{
    return x / (1.0f + 0.18f * std::abs(x));
}

float maxAbs(float a, float b)
{
    return std::max(std::abs(a), std::abs(b));
}

} // namespace

void SelfWeaponSuppressor::BandBank::reset()
{
    body.reset();
    bodyWide.reset();
    crack.reset();
    crackWide.reset();
    air.reset();
    airWide.reset();
    stepBody.reset();
    stepClarity.reset();
}

void SelfWeaponSuppressor::resetFilters()
{
    bands_.reset();
}

void SelfWeaponSuppressor::reset()
{
    resetFilters();
    filtersDirty_ = true;
    weaponOnlyMode_ = false;

    bodyFreqHz_ = 900.0f;
    bodyQ_ = 1.15f;
    crackFreqHz_ = 2450.0f;
    crackQ_ = 1.35f;
    airFreqHz_ = 7600.0f;
    airQ_ = 1.15f;
    stepBodyFreqHz_ = 1550.0f;
    stepBodyQ_ = 1.35f;
    stepClarityFreqHz_ = 3500.0f;
    stepClarityQ_ = 1.85f;

    outputTrimDb_ = 0.0f;
    ceilingAmp_ = dbToAmp(constants::kOutputCeilingDb);
    detectorAttackMs_ = 0.02f;
    detectorReleaseMs_ = 16.0f;
    holdReleaseMs_ = 36.0f;

    suppressionAmount_ = 0.0f;
    bodyAmount_ = 0.0f;
    crackAmount_ = 0.0f;
    airAmount_ = 0.0f;
    tailAmount_ = 0.0f;
    sideAmount_ = 0.0f;
    restoreAmount_ = 1.0f;
    transientAmount_ = 0.0f;
    guardAmount_ = 0.85f;
    spectralFloorAmp_ = 0.0f;
    maxCutDb_ = 96.0f;

    detectorGunMask_ = 0.0f;
    detectorProtectMask_ = 0.0f;
    activeMask_ = 0.0f;
    holdMask_ = 0.0f;
    rmsState_ = 0.0f;
    sideBackState_ = 0.0f;
    bodyEnv_ = 0.0f;
    crackEnv_ = 0.0f;
    airEnv_ = 0.0f;
    stepEnv_ = 0.0f;
    transientEnv_ = 0.0f;
    dcState_ = 0.0f;
    tailState_ = 0.0f;
    repeatState_ = 0.0f;
    sampleCounter_ = 0.0f;
    lastImpulseSample_ = -48000.0f;
    snapshot_ = {};

    updateFilters();
}

float SelfWeaponSuppressor::normalizedAmount(float value) const
{
    return clamp(value / 100.0f, 0.0f, 4.0f);
}

float SelfWeaponSuppressor::componentDrive(float value) const
{
    const float normal = saturate(value);
    const float over = clamp((value - 1.0f) / 3.0f, 0.0f, 1.0f);
    return normal + 2.15f * over;
}

float SelfWeaponSuppressor::follow(float current, float target, float attackMs, float releaseMs) const
{
    const float tauMs = target > current ? attackMs : releaseMs;
    const float alpha = std::exp(-1.0f / (constants::kSampleRate * std::max(tauMs, 0.001f) * 0.001f));
    return alpha * current + (1.0f - alpha) * target;
}

float SelfWeaponSuppressor::limit(float x) const
{
    return clamp(x, -ceilingAmp_, ceilingAmp_);
}

void SelfWeaponSuppressor::setBandTarget(float& current, float next, float tolerance)
{
    if (std::abs(current - next) > tolerance) {
        current = next;
        filtersDirty_ = true;
    }
}

void SelfWeaponSuppressor::configureBand(Biquad& filter, float frequencyHz, float q)
{
    filter.setBandPass(constants::kSampleRate, frequencyHz, q);
}

void SelfWeaponSuppressor::updateFilters()
{
    if (!filtersDirty_) {
        return;
    }

    const float bodyFreq = clamp(bodyFreqHz_, 120.0f, 3200.0f);
    const float crackFreq = clamp(crackFreqHz_, 500.0f, 9000.0f);
    const float airFreq = clamp(airFreqHz_, 2500.0f, 18000.0f);

    configureBand(bands_.body, bodyFreq, clamp(bodyQ_, 0.25f, 12.0f));
    configureBand(bands_.bodyWide, bodyFreq * 0.78f, 0.62f);
    configureBand(bands_.crack, crackFreq, clamp(crackQ_, 0.25f, 16.0f));
    configureBand(bands_.crackWide, crackFreq * 1.18f, 0.72f);
    configureBand(bands_.air, airFreq, clamp(airQ_, 0.25f, 20.0f));
    configureBand(bands_.airWide, airFreq * 0.72f, 0.66f);
    configureBand(bands_.stepBody, clamp(stepBodyFreqHz_, 120.0f, 8000.0f), clamp(stepBodyQ_, 0.25f, 12.0f));
    configureBand(bands_.stepClarity, clamp(stepClarityFreqHz_, 300.0f, 16000.0f), clamp(stepClarityQ_, 0.25f, 16.0f));

    filtersDirty_ = false;
}

void SelfWeaponSuppressor::updateTargets(const DetectorScores& scores, const EngineParams& params)
{
    weaponOnlyMode_ = params.weaponOnlyMode >= 0.5f;
    ceilingAmp_ = dbToAmp(clamp(params.outputCeilingDb, -60.0f, -0.1f));
    outputTrimDb_ = weaponOnlyMode_ ? 0.0f : clamp(params.outputTrimDb, -60.0f, 24.0f);

    const float intensity = normalizedAmount(params.changeIntensity);
    const float subtlety = saturate(params.subtletyAmount / 100.0f);
    const float legacyMute = normalizedAmount(params.weaponMuteAmount);
    const float silencer = normalizedAmount(params.weaponSilencerAmount);
    const float gunReduction = normalizedAmount(params.gunshotReduction);
    const float total = std::max({legacyMute, silencer, gunReduction}) * std::max(0.25f, intensity) *
                        lerp(1.35f, 0.62f, subtlety);

    suppressionAmount_ = weaponOnlyMode_ ? clamp(total, 0.0f, 4.0f) : 0.0f;

    const auto amountOrTotal = [this](float amount) {
        const float normalized = normalizedAmount(amount);
        return normalized > 0.001f ? normalized : suppressionAmount_;
    };

    const float midCutBoost = 1.0f + ramp(-params.weaponMidCutDb, 12.0f, 120.0f);
    const float airCutBoost = 1.0f + ramp(-params.weaponAirCutDb, 12.0f, 120.0f);
    bodyAmount_ = componentDrive(amountOrTotal(params.silencerBodyAmount) * suppressionAmount_ * 0.70f);
    crackAmount_ = componentDrive(amountOrTotal(params.silencerCrackAmount) * suppressionAmount_ * midCutBoost);
    airAmount_ = componentDrive(amountOrTotal(params.silencerAirAmount) * suppressionAmount_ * airCutBoost);
    tailAmount_ = componentDrive(amountOrTotal(params.silencerTailAmount) * suppressionAmount_);
    sideAmount_ = componentDrive(amountOrTotal(params.silencerSideAmount) * suppressionAmount_);
    restoreAmount_ = clamp(params.silencerRestoreAmount / 100.0f, 0.0f, 4.0f);
    transientAmount_ = componentDrive(normalizedAmount(params.transientKill) * std::max(0.35f, suppressionAmount_));
    guardAmount_ = std::max(saturate(params.footstepGuardAmount / 100.0f), saturate(params.protectionPasos / 100.0f));
    spectralFloorAmp_ = dbToAmp(clamp(params.spectralFloorDb, -120.0f, -6.0f));
    maxCutDb_ = clamp(params.maxCutStepDb, 3.0f, 120.0f);

    detectorAttackMs_ = clamp(params.protectionAttackMs, 0.01f, 40.0f);
    detectorReleaseMs_ = clamp(params.protectionReleaseMs, 0.01f, 240.0f);
    holdReleaseMs_ = clamp(std::max(params.sustainedHoldMs, params.stableReleaseMs), 0.5f, 3000.0f);

    const float gun = weaponOnlyMode_
        ? saturate(std::max({scores.impact, scores.protection * 0.92f, scores.action * 0.62f}))
        : 0.0f;
    const float protect = saturate(scores.footstep * guardAmount_);

    detectorGunMask_ = follow(detectorGunMask_, gun, detectorAttackMs_, detectorReleaseMs_);
    detectorProtectMask_ = follow(detectorProtectMask_, protect, 0.08f, 42.0f);

    setBandTarget(bodyFreqHz_, clamp(params.lowMidFreqHz, 120.0f, 3200.0f), 6.0f);
    setBandTarget(bodyQ_, clamp(params.lowMidQ, 0.25f, 12.0f), 0.03f);
    setBandTarget(crackFreqHz_, clamp(params.weaponMidFreqHz, 500.0f, 9000.0f), 10.0f);
    setBandTarget(crackQ_, clamp(params.weaponMidQ, 0.25f, 16.0f), 0.03f);
    setBandTarget(airFreqHz_, clamp(params.weaponAirFreqHz, 2500.0f, 18000.0f), 20.0f);
    setBandTarget(airQ_, clamp(params.weaponAirQ, 0.25f, 20.0f), 0.03f);
    setBandTarget(stepBodyFreqHz_, clamp(params.stepBodyFreqHz, 120.0f, 8000.0f), 8.0f);
    setBandTarget(stepBodyQ_, clamp(params.stepBodyQ, 0.25f, 12.0f), 0.03f);
    setBandTarget(stepClarityFreqHz_, clamp(params.stepClarityFreqHz, 300.0f, 16000.0f), 12.0f);
    setBandTarget(stepClarityQ_, clamp(params.stepClarityQ, 0.25f, 16.0f), 0.03f);
    updateFilters();
}

SelfWeaponSuppressor::FrontFrame SelfWeaponSuppressor::readFrontFrame(const float* frame,
                                                                      std::size_t channels,
                                                                      const SpatialLayout& layout) const
{
    FrontFrame out;
    const int flIdx = layout.frontLeft();
    const int frIdx = layout.frontRight();
    const int fcIdx = layout.frontCenter();

    if (flIdx >= 0 && frIdx >= 0) {
        out.left = frame[flIdx];
        out.right = frame[frIdx];
        out.frontMid = 0.5f * (out.left + out.right);
        out.frontSide = 0.5f * (out.left - out.right);
        out.hasLeftRight = true;
    } else if (channels == 1) {
        out.left = frame[0];
        out.right = frame[0];
        out.frontMid = frame[0];
        out.frontSide = 0.0f;
    } else if (channels >= 2) {
        out.left = frame[0];
        out.right = frame[1];
        out.frontMid = 0.5f * (out.left + out.right);
        out.frontSide = 0.5f * (out.left - out.right);
        out.hasLeftRight = true;
    }

    if (fcIdx >= 0) {
        out.center = frame[fcIdx];
        out.hasCenter = true;
    }

    out.centerBus = out.hasCenter ? (0.78f * out.center + 0.22f * out.frontMid) : out.frontMid;

    float sideBackSum = 0.0f;
    float sideBackCount = 0.0f;
    for (std::size_t ch = 0; ch < channels && ch < layout.roles.size(); ++ch) {
        if (layout.isSideOrBack(ch)) {
            sideBackSum += std::abs(frame[ch]);
            sideBackCount += 1.0f;
        }
    }
    out.sideBackEnergy = sideBackCount > 0.0f ? sideBackSum / sideBackCount : std::abs(out.frontSide);
    return out;
}

SelfWeaponSuppressor::BandFrame SelfWeaponSuppressor::processBands(float centerBus)
{
    updateFilters();

    BandFrame out;
    out.body = bands_.body.process(centerBus);
    out.bodyWide = bands_.bodyWide.process(centerBus);
    out.crack = bands_.crack.process(centerBus);
    out.crackWide = bands_.crackWide.process(centerBus);
    out.air = bands_.air.process(centerBus);
    out.airWide = bands_.airWide.process(centerBus);
    out.stepBody = bands_.stepBody.process(centerBus);
    out.stepClarity = bands_.stepClarity.process(centerBus);

    const float dcAlpha = std::exp(-1.0f / (constants::kSampleRate * 0.00125f));
    dcState_ = dcAlpha * dcState_ + (1.0f - dcAlpha) * centerBus;
    out.transient = centerBus - dcState_;
    return out;
}

void SelfWeaponSuppressor::writeFrontFrame(const float* input,
                                           float* output,
                                           std::size_t channels,
                                           const SpatialLayout& layout,
                                           const FrontFrame& front,
                                           float processedMid,
                                           float processedCenter,
                                           float lfeScale,
                                           float wetMix) const
{
    for (std::size_t ch = 0; ch < channels; ++ch) {
        output[ch] = input[ch];
    }

    const int flIdx = layout.frontLeft();
    const int frIdx = layout.frontRight();
    if (front.hasLeftRight && flIdx >= 0 && frIdx >= 0) {
        const float outL = processedMid + front.frontSide;
        const float outR = processedMid - front.frontSide;
        output[flIdx] = lerp(input[flIdx], outL, wetMix);
        output[frIdx] = lerp(input[frIdx], outR, wetMix);
    } else if (channels == 1) {
        output[0] = lerp(input[0], processedMid, wetMix);
    } else if (channels >= 2) {
        const float outL = processedMid + front.frontSide;
        const float outR = processedMid - front.frontSide;
        output[0] = lerp(input[0], outL, wetMix);
        output[1] = lerp(input[1], outR, wetMix);
    }

    const int fcIdx = layout.frontCenter();
    if (front.hasCenter && fcIdx >= 0) {
        output[fcIdx] = lerp(input[fcIdx], processedCenter, wetMix);
    }

    const int lfeIdx = layout.lowFrequency();
    if (lfeIdx >= 0) {
        output[lfeIdx] = lerp(input[lfeIdx], input[lfeIdx] * lfeScale, wetMix);
    }
}

void SelfWeaponSuppressor::processFrame(const float* input,
                                        float* output,
                                        std::size_t channels,
                                        const SpatialLayout& layout,
                                        float wetMix,
                                        float& peak)
{
    if (!input || !output || channels == 0) {
        return;
    }

    wetMix = clamp(wetMix, 0.0f, 1.0f);
    const FrontFrame front = readFrontFrame(input, channels, layout);
    const BandFrame bands = processBands(front.centerBus);

    rmsState_ = follow(rmsState_, front.centerBus * front.centerBus, 0.12f, 180.0f);
    sideBackState_ = follow(sideBackState_, front.sideBackEnergy, 0.12f, 64.0f);
    const float adaptive = std::max(0.00005f, std::sqrt(rmsState_ + constants::kEpsEnergy) * 0.36f);

    bodyEnv_ = follow(bodyEnv_, 0.70f * std::abs(bands.body) + 0.30f * std::abs(bands.bodyWide), 0.012f, 24.0f);
    crackEnv_ = follow(crackEnv_, 0.58f * std::abs(bands.crack) + 0.42f * std::abs(bands.crackWide), 0.006f, 16.0f);
    airEnv_ = follow(airEnv_, 0.58f * std::abs(bands.air) + 0.42f * std::abs(bands.airWide), 0.006f, 14.0f);
    stepEnv_ = follow(stepEnv_, 0.52f * std::abs(bands.stepBody) + 0.48f * std::abs(bands.stepClarity), 0.20f, 82.0f);
    transientEnv_ = follow(transientEnv_, std::abs(bands.transient), 0.004f, 9.0f);

    const float bodySig = ramp(bodyEnv_, adaptive * 0.65f, adaptive * 3.70f);
    const float crackSig = ramp(crackEnv_, adaptive * 0.52f, adaptive * 4.15f);
    const float airSig = ramp(airEnv_, adaptive * 0.48f, adaptive * 3.95f);
    const float stepSig = ramp(stepEnv_, adaptive * 0.72f, adaptive * 3.35f);
    const float transientSig = ramp(transientEnv_, adaptive * 1.02f, adaptive * 7.00f);
    const float sideBackSig = ramp(sideBackState_, adaptive * 0.72f, adaptive * 4.80f);

    sampleCounter_ += 1.0f;
    const bool impulse = transientSig > 0.82f && (crackSig > 0.46f || airSig > 0.46f);
    if (impulse) {
        const float deltaSamples = sampleCounter_ - lastImpulseSample_;
        const float deltaMs = 1000.0f * deltaSamples / constants::kSampleRate;
        const float autoFire = ramp(deltaMs, 28.0f, 48.0f) * (1.0f - ramp(deltaMs, 165.0f, 260.0f));
        repeatState_ = std::max(repeatState_, autoFire);
        lastImpulseSample_ = sampleCounter_;
    }
    repeatState_ = follow(repeatState_, 0.0f, 1.0f, 120.0f);

    const float crackAir = std::sqrt(std::max(0.0f, crackSig * airSig));
    const float broadband = std::min(std::max(bodySig, crackSig * 0.80f), std::max(airSig, crackSig));
    const float centered = saturate(1.0f - 0.30f * sideBackSig);
    const float footProtect = saturate(std::max(detectorProtectMask_, stepSig * (0.40f + 0.42f * sideBackSig)) *
                                       guardAmount_ * (1.0f - 0.56f * crackAir * transientSig));

    const float localWeapon = saturate((0.31f * transientSig + 0.25f * crackAir + 0.20f * broadband +
                                        0.13f * std::max(crackSig, airSig) + 0.11f * repeatState_) *
                                       centered);

    const float targetMask = saturate(std::max(detectorGunMask_, localWeapon) * saturate(0.45f + suppressionAmount_ * 0.42f));
    activeMask_ = follow(activeMask_, targetMask, detectorAttackMs_, detectorReleaseMs_);
    holdMask_ = follow(holdMask_, activeMask_, 0.006f, holdReleaseMs_);

    const float transientWeight = 0.35f + 0.65f * std::max(transientSig, crackAir);
    const float mask = saturate(std::max(activeMask_, holdMask_ * saturate(tailAmount_ * 0.26f)) * transientWeight);
    const float protectRestore = saturate(footProtect * restoreAmount_ * 0.34f);
    const float overdrive = clamp((suppressionAmount_ - 1.0f) / 3.0f, 0.0f, 1.0f);

    const float tailAlpha = std::exp(-1.0f / (constants::kSampleRate * 0.0075f));
    tailState_ = tailAlpha * tailState_ + (1.0f - tailAlpha) *
                                      (0.46f * bands.body + 0.28f * bands.crack + 0.18f * bands.air +
                                       0.08f * bands.bodyWide);

    const float weaponEstimate =
        bands.body * clamp(bodyAmount_ * 0.58f, 0.0f, 2.10f) +
        bands.bodyWide * clamp(bodyAmount_ * 0.26f, 0.0f, 1.20f) +
        bands.crack * clamp(crackAmount_ * 0.92f, 0.0f, 4.50f) +
        bands.crackWide * clamp(crackAmount_ * 0.46f, 0.0f, 2.80f) +
        bands.air * clamp(airAmount_ * 0.86f, 0.0f, 4.20f) +
        bands.airWide * clamp(airAmount_ * 0.38f, 0.0f, 2.40f) +
        bands.transient * clamp(transientAmount_ * 0.78f, 0.0f, 4.20f) +
        tailState_ * clamp(tailAmount_ * 0.68f, 0.0f, 3.00f);

    const float frontStrength = front.hasCenter ? 0.72f : 1.0f;
    const float centerStrength = front.hasCenter ? 1.26f : 1.0f;
    const float sideLeakStrength = clamp(sideAmount_ * 0.10f, 0.0f, 0.35f) * mask;

    float processedMid = front.frontMid - weaponEstimate * mask * frontStrength;
    float processedCenter = front.hasCenter ? front.center - weaponEstimate * mask * centerStrength : processedMid;

    const float directMask = saturate(mask * overdrive * (0.52f * transientSig + 0.32f * crackAir + 0.16f * broadband));
    const float maxDirectCutDb = -clamp(maxCutDb_ + 24.0f * overdrive, 12.0f, 144.0f);
    const float directGain = dbToAmp(maxDirectCutDb * directMask);
    const float residue = signedSoftClip(bands.body * spectralFloorAmp_ * (0.18f + 0.10f * restoreAmount_) * mask);

    processedMid = lerp(processedMid, processedMid * directGain + residue, directMask * 0.82f);
    processedCenter = lerp(processedCenter, processedCenter * directGain + residue, directMask);

    const float stepRestore = (0.28f * bands.stepBody + 0.42f * bands.stepClarity) * footProtect *
                              saturate(restoreAmount_) * (1.0f - 0.35f * directMask);
    processedMid += stepRestore;
    processedCenter += stepRestore * 0.45f;

    processedMid = lerp(processedMid, front.frontMid, protectRestore);
    processedCenter = lerp(processedCenter, front.center, protectRestore * 0.80f);

    float lfeScale = 1.0f;
    if (layout.lowFrequency() >= 0) {
        lfeScale = lerp(1.0f, dbToAmp(-36.0f - 42.0f * overdrive), saturate(mask * bodySig * (0.55f + 0.45f * overdrive)));
    }

    writeFrontFrame(input, output, channels, layout, front, processedMid, processedCenter, lfeScale, wetMix);

    if (sideLeakStrength > 0.0001f) {
        for (std::size_t ch = 0; ch < channels && ch < layout.roles.size(); ++ch) {
            if (layout.isSideOrBack(ch)) {
                output[ch] = lerp(output[ch], input[ch] - weaponEstimate * sideLeakStrength, wetMix);
            }
        }
    }

    const float gain = dbToAmp(outputTrimDb_);
    for (std::size_t ch = 0; ch < channels; ++ch) {
        output[ch] = limit(output[ch] * gain);
        peak = std::max(peak, std::abs(output[ch]));
    }

    snapshot_.weaponMask = mask;
    snapshot_.protectMask = footProtect;
    snapshot_.centerConfidence = centered;
    snapshot_.outputPeak = peak;
}

} // namespace warzone_audio
