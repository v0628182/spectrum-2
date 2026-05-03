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

    try {
        left_.assign(maxFrames, 0.0f);
        right_.assign(maxFrames, 0.0f);
        outLeft_.assign(maxFrames, 0.0f);
        outRight_.assign(maxFrames, 0.0f);
        maxFramesPrepared_ = maxFrames;
        maxChannelsPrepared_ = maxChannels;
        return true;
    } catch (...) {
        left_.clear();
        right_.clear();
        outLeft_.clear();
        outRight_.clear();
        maxFramesPrepared_ = 0;
        maxChannelsPrepared_ = 0;
        return false;
    }
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
                                        std::size_t channels) noexcept
{
    if (!input || !output || frames == 0 || channels == 0) {
        return;
    }

    lastFrames_.store(frames, std::memory_order_relaxed);
    lastChannels_.store(channels, std::memory_order_relaxed);

    if (maxFramesPrepared_ == 0 || frames > maxFramesPrepared_) {
        oversizedBlocks_.fetch_add(1, std::memory_order_relaxed);
        passthrough(input, output, frames, channels);
        return;
    }

    applyPendingParamsIfNeeded();

    for (std::size_t i = 0; i < frames; ++i) {
        const float* frame = input + i * channels;
        if (channels == 1) {
            left_[i] = frame[0];
            right_[i] = frame[0];
        } else if (channels == 2) {
            left_[i] = frame[0];
            right_[i] = frame[1];
        } else {
            const float fl = frame[0];
            const float fr = frame[1];
            const float fc = channels > 2 ? frame[2] : 0.0f;
            const float lfe = channels > 3 ? frame[3] : 0.0f;
            const float bl = channels > 4 ? frame[4] : 0.0f;
            const float br = channels > 5 ? frame[5] : 0.0f;
            const float sl = channels > 6 ? frame[6] : 0.0f;
            const float sr = channels > 7 ? frame[7] : 0.0f;
            left_[i] = fl + 0.7071f * fc + 0.25f * lfe + 0.7071f * bl + 0.7071f * sl;
            right_[i] = fr + 0.7071f * fc + 0.25f * lfe + 0.7071f * br + 0.7071f * sr;
            const float peak = std::max(std::abs(left_[i]), std::abs(right_[i]));
            if (peak > 1.0f) {
                left_[i] /= peak;
                right_[i] /= peak;
            }
        }
    }

    engine_.processBlock(left_.data(), right_.data(), outLeft_.data(), outRight_.data(), frames);

    for (std::size_t i = 0; i < frames; ++i) {
        float* frame = output + i * channels;
        if (channels == 1) {
            frame[0] = 0.5f * (outLeft_[i] + outRight_[i]);
        } else {
            frame[0] = outLeft_[i];
            frame[1] = outRight_[i];
            for (std::size_t ch = 2; ch < channels; ++ch) {
                frame[ch] = 0.0f;
            }
        }
    }

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
