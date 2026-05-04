#include "warzone_audio/RealtimeEngine.h"

#include <algorithm>
#include <cmath>

namespace warzone_audio {

namespace {

void copyIfNeeded(const float* input, float* output, std::size_t samples) noexcept
{
    if (input != output) {
        std::copy(input, input + samples, output);
    }
}

float sanitize(float value, float fallback) noexcept
{
    return std::isfinite(value) ? value : fallback;
}

} // namespace

RealtimeEngine::AtomicParams::AtomicParams() noexcept
    : footstepEnhance(EngineParams{}.footstepEnhance),
      actionDetail(EngineParams{}.actionDetail),
      gunshotReduction(EngineParams{}.gunshotReduction),
      explosionReduction(EngineParams{}.explosionReduction),
      detectionSensitivity(EngineParams{}.detectionSensitivity),
      outputCeilingDb(EngineParams{}.outputCeilingDb),
      stepBodyBoostDb(EngineParams{}.stepBodyBoostDb),
      stepClarityBoostDb(EngineParams{}.stepClarityBoostDb),
      stepLowBodyBoostDb(EngineParams{}.stepLowBodyBoostDb),
      stepLowMidBoostDb(EngineParams{}.stepLowMidBoostDb),
      weaponMidCutDb(EngineParams{}.weaponMidCutDb),
      weaponAirCutDb(EngineParams{}.weaponAirCutDb),
      sustainedHoldMs(EngineParams{}.sustainedHoldMs),
      masterDuckDb(EngineParams{}.masterDuckDb),
      impactDuckDb(EngineParams{}.impactDuckDb),
      footstepLevelerAmount(EngineParams{}.footstepLevelerAmount),
      footstepTargetRmsDb(EngineParams{}.footstepTargetRmsDb),
      footstepMaxLiftDb(EngineParams{}.footstepMaxLiftDb),
      footstepLevelerSpeedMs(EngineParams{}.footstepLevelerSpeedMs),
      stabilityAmount(EngineParams{}.stabilityAmount),
      spectralFloorDb(EngineParams{}.spectralFloorDb),
      stableReleaseMs(EngineParams{}.stableReleaseMs),
      footstepGuardAmount(EngineParams{}.footstepGuardAmount),
      maxCutStepDb(EngineParams{}.maxCutStepDb),
      transientKill(EngineParams{}.transientKill),
      lookaheadMs(EngineParams{}.lookaheadMs),
      outputTrimDb(EngineParams{}.outputTrimDb),
      residualReductionDb(EngineParams{}.residualReductionDb),
      balanceLowDb(EngineParams{}.balanceLowDb),
      balanceMidDb(EngineParams{}.balanceMidDb),
      balanceHighDb(EngineParams{}.balanceHighDb),
      stftCutoffHz(EngineParams{}.stftCutoffHz),
      stftPreserveDb(EngineParams{}.stftPreserveDb),
      spectralFloorStab(EngineParams{}.spectralFloorStab),
      protectionPasos(EngineParams{}.protectionPasos),
      weaponOnlyMode(EngineParams{}.weaponOnlyMode),
      changeIntensity(EngineParams{}.changeIntensity),
      subtletyAmount(EngineParams{}.subtletyAmount),
      wetMix(EngineParams{}.wetMix),
      lowShelfFreqHz(EngineParams{}.lowShelfFreqHz),
      lowMidFreqHz(EngineParams{}.lowMidFreqHz),
      lowMidQ(EngineParams{}.lowMidQ),
      weaponMidFreqHz(EngineParams{}.weaponMidFreqHz),
      weaponMidQ(EngineParams{}.weaponMidQ),
      stepBodyFreqHz(EngineParams{}.stepBodyFreqHz),
      stepBodyQ(EngineParams{}.stepBodyQ),
      stepClarityFreqHz(EngineParams{}.stepClarityFreqHz),
      stepClarityQ(EngineParams{}.stepClarityQ),
      weaponAirFreqHz(EngineParams{}.weaponAirFreqHz),
      weaponAirQ(EngineParams{}.weaponAirQ),
      protectionAttackMs(EngineParams{}.protectionAttackMs),
      protectionReleaseMs(EngineParams{}.protectionReleaseMs),
      boostAttackMs(EngineParams{}.boostAttackMs),
      boostReleaseMs(EngineParams{}.boostReleaseMs),
      limiterReleaseMs(EngineParams{}.limiterReleaseMs),
      stereoWidth(EngineParams{}.stereoWidth),
      weaponMuteAmount(EngineParams{}.weaponMuteAmount),
      weaponSilencerAmount(EngineParams{}.weaponSilencerAmount),
      silencerBodyAmount(EngineParams{}.silencerBodyAmount),
      silencerCrackAmount(EngineParams{}.silencerCrackAmount),
      silencerAirAmount(EngineParams{}.silencerAirAmount),
      silencerTailAmount(EngineParams{}.silencerTailAmount),
      silencerSideAmount(EngineParams{}.silencerSideAmount),
      silencerRestoreAmount(EngineParams{}.silencerRestoreAmount),
      protectionExtreme(EngineParams{}.protectionExtreme ? 1 : 0),
      spectralMaskEnabled(EngineParams{}.spectralMaskEnabled ? 1 : 0),
      debugLogging(EngineParams{}.debugLogging ? 1 : 0)
{
}

void RealtimeEngine::AtomicParams::store(const EngineParams& params) noexcept
{
    footstepEnhance.store(sanitize(params.footstepEnhance, EngineParams{}.footstepEnhance), std::memory_order_relaxed);
    actionDetail.store(sanitize(params.actionDetail, EngineParams{}.actionDetail), std::memory_order_relaxed);
    gunshotReduction.store(sanitize(params.gunshotReduction, EngineParams{}.gunshotReduction), std::memory_order_relaxed);
    explosionReduction.store(sanitize(params.explosionReduction, EngineParams{}.explosionReduction), std::memory_order_relaxed);
    detectionSensitivity.store(sanitize(params.detectionSensitivity, EngineParams{}.detectionSensitivity), std::memory_order_relaxed);
    outputCeilingDb.store(sanitize(params.outputCeilingDb, EngineParams{}.outputCeilingDb), std::memory_order_relaxed);
    stepBodyBoostDb.store(sanitize(params.stepBodyBoostDb, EngineParams{}.stepBodyBoostDb), std::memory_order_relaxed);
    stepClarityBoostDb.store(sanitize(params.stepClarityBoostDb, EngineParams{}.stepClarityBoostDb), std::memory_order_relaxed);
    stepLowBodyBoostDb.store(sanitize(params.stepLowBodyBoostDb, EngineParams{}.stepLowBodyBoostDb), std::memory_order_relaxed);
    stepLowMidBoostDb.store(sanitize(params.stepLowMidBoostDb, EngineParams{}.stepLowMidBoostDb), std::memory_order_relaxed);
    weaponMidCutDb.store(sanitize(params.weaponMidCutDb, EngineParams{}.weaponMidCutDb), std::memory_order_relaxed);
    weaponAirCutDb.store(sanitize(params.weaponAirCutDb, EngineParams{}.weaponAirCutDb), std::memory_order_relaxed);
    sustainedHoldMs.store(sanitize(params.sustainedHoldMs, EngineParams{}.sustainedHoldMs), std::memory_order_relaxed);
    masterDuckDb.store(sanitize(params.masterDuckDb, EngineParams{}.masterDuckDb), std::memory_order_relaxed);
    impactDuckDb.store(sanitize(params.impactDuckDb, EngineParams{}.impactDuckDb), std::memory_order_relaxed);
    footstepLevelerAmount.store(sanitize(params.footstepLevelerAmount, EngineParams{}.footstepLevelerAmount), std::memory_order_relaxed);
    footstepTargetRmsDb.store(sanitize(params.footstepTargetRmsDb, EngineParams{}.footstepTargetRmsDb), std::memory_order_relaxed);
    footstepMaxLiftDb.store(sanitize(params.footstepMaxLiftDb, EngineParams{}.footstepMaxLiftDb), std::memory_order_relaxed);
    footstepLevelerSpeedMs.store(sanitize(params.footstepLevelerSpeedMs, EngineParams{}.footstepLevelerSpeedMs), std::memory_order_relaxed);
    stabilityAmount.store(sanitize(params.stabilityAmount, EngineParams{}.stabilityAmount), std::memory_order_relaxed);
    spectralFloorDb.store(sanitize(params.spectralFloorDb, EngineParams{}.spectralFloorDb), std::memory_order_relaxed);
    stableReleaseMs.store(sanitize(params.stableReleaseMs, EngineParams{}.stableReleaseMs), std::memory_order_relaxed);
    footstepGuardAmount.store(sanitize(params.footstepGuardAmount, EngineParams{}.footstepGuardAmount), std::memory_order_relaxed);
    maxCutStepDb.store(sanitize(params.maxCutStepDb, EngineParams{}.maxCutStepDb), std::memory_order_relaxed);
    transientKill.store(sanitize(params.transientKill, EngineParams{}.transientKill), std::memory_order_relaxed);
    lookaheadMs.store(sanitize(params.lookaheadMs, EngineParams{}.lookaheadMs), std::memory_order_relaxed);
    outputTrimDb.store(sanitize(params.outputTrimDb, EngineParams{}.outputTrimDb), std::memory_order_relaxed);
    residualReductionDb.store(sanitize(params.residualReductionDb, EngineParams{}.residualReductionDb), std::memory_order_relaxed);
    balanceLowDb.store(sanitize(params.balanceLowDb, EngineParams{}.balanceLowDb), std::memory_order_relaxed);
    balanceMidDb.store(sanitize(params.balanceMidDb, EngineParams{}.balanceMidDb), std::memory_order_relaxed);
    balanceHighDb.store(sanitize(params.balanceHighDb, EngineParams{}.balanceHighDb), std::memory_order_relaxed);
    stftCutoffHz.store(sanitize(params.stftCutoffHz, EngineParams{}.stftCutoffHz), std::memory_order_relaxed);
    stftPreserveDb.store(sanitize(params.stftPreserveDb, EngineParams{}.stftPreserveDb), std::memory_order_relaxed);
    spectralFloorStab.store(sanitize(params.spectralFloorStab, EngineParams{}.spectralFloorStab), std::memory_order_relaxed);
    protectionPasos.store(sanitize(params.protectionPasos, EngineParams{}.protectionPasos), std::memory_order_relaxed);
    weaponOnlyMode.store(sanitize(params.weaponOnlyMode, EngineParams{}.weaponOnlyMode), std::memory_order_relaxed);
    changeIntensity.store(sanitize(params.changeIntensity, EngineParams{}.changeIntensity), std::memory_order_relaxed);
    subtletyAmount.store(sanitize(params.subtletyAmount, EngineParams{}.subtletyAmount), std::memory_order_relaxed);
    wetMix.store(sanitize(params.wetMix, EngineParams{}.wetMix), std::memory_order_relaxed);
    lowShelfFreqHz.store(sanitize(params.lowShelfFreqHz, EngineParams{}.lowShelfFreqHz), std::memory_order_relaxed);
    lowMidFreqHz.store(sanitize(params.lowMidFreqHz, EngineParams{}.lowMidFreqHz), std::memory_order_relaxed);
    lowMidQ.store(sanitize(params.lowMidQ, EngineParams{}.lowMidQ), std::memory_order_relaxed);
    weaponMidFreqHz.store(sanitize(params.weaponMidFreqHz, EngineParams{}.weaponMidFreqHz), std::memory_order_relaxed);
    weaponMidQ.store(sanitize(params.weaponMidQ, EngineParams{}.weaponMidQ), std::memory_order_relaxed);
    stepBodyFreqHz.store(sanitize(params.stepBodyFreqHz, EngineParams{}.stepBodyFreqHz), std::memory_order_relaxed);
    stepBodyQ.store(sanitize(params.stepBodyQ, EngineParams{}.stepBodyQ), std::memory_order_relaxed);
    stepClarityFreqHz.store(sanitize(params.stepClarityFreqHz, EngineParams{}.stepClarityFreqHz), std::memory_order_relaxed);
    stepClarityQ.store(sanitize(params.stepClarityQ, EngineParams{}.stepClarityQ), std::memory_order_relaxed);
    weaponAirFreqHz.store(sanitize(params.weaponAirFreqHz, EngineParams{}.weaponAirFreqHz), std::memory_order_relaxed);
    weaponAirQ.store(sanitize(params.weaponAirQ, EngineParams{}.weaponAirQ), std::memory_order_relaxed);
    protectionAttackMs.store(sanitize(params.protectionAttackMs, EngineParams{}.protectionAttackMs), std::memory_order_relaxed);
    protectionReleaseMs.store(sanitize(params.protectionReleaseMs, EngineParams{}.protectionReleaseMs), std::memory_order_relaxed);
    boostAttackMs.store(sanitize(params.boostAttackMs, EngineParams{}.boostAttackMs), std::memory_order_relaxed);
    boostReleaseMs.store(sanitize(params.boostReleaseMs, EngineParams{}.boostReleaseMs), std::memory_order_relaxed);
    limiterReleaseMs.store(sanitize(params.limiterReleaseMs, EngineParams{}.limiterReleaseMs), std::memory_order_relaxed);
    stereoWidth.store(sanitize(params.stereoWidth, EngineParams{}.stereoWidth), std::memory_order_relaxed);
    weaponMuteAmount.store(sanitize(params.weaponMuteAmount, EngineParams{}.weaponMuteAmount), std::memory_order_relaxed);
    weaponSilencerAmount.store(sanitize(params.weaponSilencerAmount, EngineParams{}.weaponSilencerAmount), std::memory_order_relaxed);
    silencerBodyAmount.store(sanitize(params.silencerBodyAmount, EngineParams{}.silencerBodyAmount), std::memory_order_relaxed);
    silencerCrackAmount.store(sanitize(params.silencerCrackAmount, EngineParams{}.silencerCrackAmount), std::memory_order_relaxed);
    silencerAirAmount.store(sanitize(params.silencerAirAmount, EngineParams{}.silencerAirAmount), std::memory_order_relaxed);
    silencerTailAmount.store(sanitize(params.silencerTailAmount, EngineParams{}.silencerTailAmount), std::memory_order_relaxed);
    silencerSideAmount.store(sanitize(params.silencerSideAmount, EngineParams{}.silencerSideAmount), std::memory_order_relaxed);
    silencerRestoreAmount.store(sanitize(params.silencerRestoreAmount, EngineParams{}.silencerRestoreAmount), std::memory_order_relaxed);
    protectionExtreme.store(params.protectionExtreme ? 1 : 0, std::memory_order_relaxed);
    spectralMaskEnabled.store(params.spectralMaskEnabled ? 1 : 0, std::memory_order_relaxed);
    debugLogging.store(params.debugLogging ? 1 : 0, std::memory_order_relaxed);
}

EngineParams RealtimeEngine::AtomicParams::load() const noexcept
{
    EngineParams params;
    params.footstepEnhance = footstepEnhance.load(std::memory_order_relaxed);
    params.actionDetail = actionDetail.load(std::memory_order_relaxed);
    params.gunshotReduction = gunshotReduction.load(std::memory_order_relaxed);
    params.explosionReduction = explosionReduction.load(std::memory_order_relaxed);
    params.detectionSensitivity = detectionSensitivity.load(std::memory_order_relaxed);
    params.outputCeilingDb = outputCeilingDb.load(std::memory_order_relaxed);
    params.stepBodyBoostDb = stepBodyBoostDb.load(std::memory_order_relaxed);
    params.stepClarityBoostDb = stepClarityBoostDb.load(std::memory_order_relaxed);
    params.stepLowBodyBoostDb = stepLowBodyBoostDb.load(std::memory_order_relaxed);
    params.stepLowMidBoostDb = stepLowMidBoostDb.load(std::memory_order_relaxed);
    params.weaponMidCutDb = weaponMidCutDb.load(std::memory_order_relaxed);
    params.weaponAirCutDb = weaponAirCutDb.load(std::memory_order_relaxed);
    params.sustainedHoldMs = sustainedHoldMs.load(std::memory_order_relaxed);
    params.masterDuckDb = masterDuckDb.load(std::memory_order_relaxed);
    params.impactDuckDb = impactDuckDb.load(std::memory_order_relaxed);
    params.footstepLevelerAmount = footstepLevelerAmount.load(std::memory_order_relaxed);
    params.footstepTargetRmsDb = footstepTargetRmsDb.load(std::memory_order_relaxed);
    params.footstepMaxLiftDb = footstepMaxLiftDb.load(std::memory_order_relaxed);
    params.footstepLevelerSpeedMs = footstepLevelerSpeedMs.load(std::memory_order_relaxed);
    params.stabilityAmount = stabilityAmount.load(std::memory_order_relaxed);
    params.spectralFloorDb = spectralFloorDb.load(std::memory_order_relaxed);
    params.stableReleaseMs = stableReleaseMs.load(std::memory_order_relaxed);
    params.footstepGuardAmount = footstepGuardAmount.load(std::memory_order_relaxed);
    params.maxCutStepDb = maxCutStepDb.load(std::memory_order_relaxed);
    params.transientKill = transientKill.load(std::memory_order_relaxed);
    params.lookaheadMs = lookaheadMs.load(std::memory_order_relaxed);
    params.outputTrimDb = outputTrimDb.load(std::memory_order_relaxed);
    params.residualReductionDb = residualReductionDb.load(std::memory_order_relaxed);
    params.balanceLowDb = balanceLowDb.load(std::memory_order_relaxed);
    params.balanceMidDb = balanceMidDb.load(std::memory_order_relaxed);
    params.balanceHighDb = balanceHighDb.load(std::memory_order_relaxed);
    params.stftCutoffHz = stftCutoffHz.load(std::memory_order_relaxed);
    params.stftPreserveDb = stftPreserveDb.load(std::memory_order_relaxed);
    params.spectralFloorStab = spectralFloorStab.load(std::memory_order_relaxed);
    params.protectionPasos = protectionPasos.load(std::memory_order_relaxed);
    params.weaponOnlyMode = weaponOnlyMode.load(std::memory_order_relaxed);
    params.changeIntensity = changeIntensity.load(std::memory_order_relaxed);
    params.subtletyAmount = subtletyAmount.load(std::memory_order_relaxed);
    params.wetMix = wetMix.load(std::memory_order_relaxed);
    params.lowShelfFreqHz = lowShelfFreqHz.load(std::memory_order_relaxed);
    params.lowMidFreqHz = lowMidFreqHz.load(std::memory_order_relaxed);
    params.lowMidQ = lowMidQ.load(std::memory_order_relaxed);
    params.weaponMidFreqHz = weaponMidFreqHz.load(std::memory_order_relaxed);
    params.weaponMidQ = weaponMidQ.load(std::memory_order_relaxed);
    params.stepBodyFreqHz = stepBodyFreqHz.load(std::memory_order_relaxed);
    params.stepBodyQ = stepBodyQ.load(std::memory_order_relaxed);
    params.stepClarityFreqHz = stepClarityFreqHz.load(std::memory_order_relaxed);
    params.stepClarityQ = stepClarityQ.load(std::memory_order_relaxed);
    params.weaponAirFreqHz = weaponAirFreqHz.load(std::memory_order_relaxed);
    params.weaponAirQ = weaponAirQ.load(std::memory_order_relaxed);
    params.protectionAttackMs = protectionAttackMs.load(std::memory_order_relaxed);
    params.protectionReleaseMs = protectionReleaseMs.load(std::memory_order_relaxed);
    params.boostAttackMs = boostAttackMs.load(std::memory_order_relaxed);
    params.boostReleaseMs = boostReleaseMs.load(std::memory_order_relaxed);
    params.limiterReleaseMs = limiterReleaseMs.load(std::memory_order_relaxed);
    params.stereoWidth = stereoWidth.load(std::memory_order_relaxed);
    params.weaponMuteAmount = weaponMuteAmount.load(std::memory_order_relaxed);
    params.weaponSilencerAmount = weaponSilencerAmount.load(std::memory_order_relaxed);
    params.silencerBodyAmount = silencerBodyAmount.load(std::memory_order_relaxed);
    params.silencerCrackAmount = silencerCrackAmount.load(std::memory_order_relaxed);
    params.silencerAirAmount = silencerAirAmount.load(std::memory_order_relaxed);
    params.silencerTailAmount = silencerTailAmount.load(std::memory_order_relaxed);
    params.silencerSideAmount = silencerSideAmount.load(std::memory_order_relaxed);
    params.silencerRestoreAmount = silencerRestoreAmount.load(std::memory_order_relaxed);
    params.protectionExtreme = protectionExtreme.load(std::memory_order_relaxed) != 0;
    params.spectralMaskEnabled = spectralMaskEnabled.load(std::memory_order_relaxed) != 0;
    params.debugLogging = debugLogging.load(std::memory_order_relaxed) != 0;
    return params;
}

RealtimeEngine::RealtimeEngine()
{
    cachedParams_ = pendingParams_.load();
    engine_.setParams(cachedParams_);
}

bool RealtimeEngine::prepare(std::size_t maxFrames, std::size_t maxChannels)
{
    if (maxFrames == 0 || maxChannels == 0) {
        return false;
    }

    maxFramesPrepared_ = maxFrames;
    maxChannelsPrepared_ = maxChannels;
    return true;
}

void RealtimeEngine::reset()
{
    engine_.reset();
    appliedRevision_ = 0;
    appliedUpdates_ = 0;
    snapshotFootstep_.store(0.0f, std::memory_order_relaxed);
    snapshotAction_.store(0.0f, std::memory_order_relaxed);
    snapshotProtection_.store(0.0f, std::memory_order_relaxed);
    snapshotLateral_.store(0.0f, std::memory_order_relaxed);
    snapshotConfidence_.store(0.0f, std::memory_order_relaxed);
    snapshotOutputPeak_.store(0.0f, std::memory_order_relaxed);
    snapshotFramesAnalyzed_.store(0, std::memory_order_relaxed);
    blocksProcessed_.store(0, std::memory_order_relaxed);
    parameterUpdatesApplied_.store(0, std::memory_order_relaxed);
    bypassedBlocks_.store(0, std::memory_order_relaxed);
    oversizedBlocks_.store(0, std::memory_order_relaxed);
    lastFrames_.store(0, std::memory_order_relaxed);
    lastChannels_.store(0, std::memory_order_relaxed);
}

void RealtimeEngine::setParams(const EngineParams& params) noexcept
{
    pendingParams_.store(params);
    pendingRevision_.fetch_add(1, std::memory_order_release);
}

EngineParams RealtimeEngine::paramsSnapshot() const noexcept
{
    return pendingParams_.load();
}

void RealtimeEngine::applyPendingParamsIfNeeded() noexcept
{
    const auto revision = pendingRevision_.load(std::memory_order_acquire);
    if (revision != appliedRevision_) {
        cachedParams_ = pendingParams_.load();
        engine_.setParams(cachedParams_);
        appliedRevision_ = revision;
        ++appliedUpdates_;
        parameterUpdatesApplied_.store(appliedUpdates_, std::memory_order_release);
    }
}

void RealtimeEngine::passthrough(const float* input, float* output, std::size_t frames, std::size_t channels) noexcept
{
    copyIfNeeded(input, output, frames * channels);
    bypassedBlocks_.fetch_add(1, std::memory_order_relaxed);
}

void RealtimeEngine::processInterleaved(const float* input,
                                        float* output,
                                        std::size_t frames,
                                        std::size_t channels,
                                        std::uint32_t channelMask) noexcept
{
    if (!input || !output || frames == 0 || channels == 0) {
        return;
    }

    lastFrames_.store(frames, std::memory_order_relaxed);
    lastChannels_.store(channels, std::memory_order_relaxed);

    if (maxFramesPrepared_ == 0 || frames > maxFramesPrepared_ ||
        maxChannelsPrepared_ == 0 || channels > maxChannelsPrepared_) {
        oversizedBlocks_.fetch_add(1, std::memory_order_relaxed);
        passthrough(input, output, frames, channels);
        return;
    }

    applyPendingParamsIfNeeded();
    engine_.processInterleaved(input, output, frames, channels, channelMask);

    blocksProcessed_.fetch_add(1, std::memory_order_relaxed);
    publishSnapshot();
}

void RealtimeEngine::publishSnapshot() noexcept
{
    const auto& stats = engine_.stats();
    snapshotFootstep_.store(stats.scores.footstep, std::memory_order_relaxed);
    snapshotAction_.store(stats.scores.action, std::memory_order_relaxed);
    snapshotProtection_.store(stats.scores.protection, std::memory_order_relaxed);
    snapshotLateral_.store(stats.scores.lateral, std::memory_order_relaxed);
    snapshotConfidence_.store(stats.scores.confidence, std::memory_order_relaxed);
    snapshotOutputPeak_.store(stats.outputPeak, std::memory_order_relaxed);
    snapshotFramesAnalyzed_.store(static_cast<unsigned long long>(stats.framesAnalyzed), std::memory_order_release);
}

RealtimeSnapshot RealtimeEngine::snapshot() const noexcept
{
    RealtimeSnapshot out;
    out.footstep = snapshotFootstep_.load(std::memory_order_relaxed);
    out.action = snapshotAction_.load(std::memory_order_relaxed);
    out.protection = snapshotProtection_.load(std::memory_order_relaxed);
    out.lateral = snapshotLateral_.load(std::memory_order_relaxed);
    out.confidence = snapshotConfidence_.load(std::memory_order_relaxed);
    out.outputPeak = snapshotOutputPeak_.load(std::memory_order_relaxed);
    out.framesAnalyzed = snapshotFramesAnalyzed_.load(std::memory_order_acquire);
    out.blocksProcessed = blocksProcessed_.load(std::memory_order_relaxed);
    out.parameterUpdatesApplied = parameterUpdatesApplied_.load(std::memory_order_relaxed);
    out.bypassedBlocks = bypassedBlocks_.load(std::memory_order_relaxed);
    out.oversizedBlocks = oversizedBlocks_.load(std::memory_order_relaxed);
    out.maxFramesPrepared = maxFramesPrepared_;
    out.lastFrames = lastFrames_.load(std::memory_order_relaxed);
    out.lastChannels = lastChannels_.load(std::memory_order_relaxed);
    return out;
}

} // namespace warzone_audio
