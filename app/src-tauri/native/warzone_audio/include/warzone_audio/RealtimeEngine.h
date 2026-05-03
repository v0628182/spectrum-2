#pragma once

#include <atomic>
#include <cstddef>
#include <vector>

#include "warzone_audio/DspEngine.h"
#include "warzone_audio/DspTypes.h"

namespace warzone_audio {

struct RealtimeSnapshot {
    float footstep = 0.0f;
    float action = 0.0f;
    float protection = 0.0f;
    float lateral = 0.0f;
    float confidence = 0.0f;
    float outputPeak = 0.0f;
    unsigned long long framesAnalyzed = 0;
    unsigned long long blocksProcessed = 0;
    unsigned long long parameterUpdatesApplied = 0;
    unsigned long long bypassedBlocks = 0;
    unsigned long long oversizedBlocks = 0;
    std::size_t maxFramesPrepared = 0;
    std::size_t lastFrames = 0;
    std::size_t lastChannels = 0;
};

class RealtimeEngine {
public:
    RealtimeEngine();

    RealtimeEngine(const RealtimeEngine&) = delete;
    RealtimeEngine& operator=(const RealtimeEngine&) = delete;
    RealtimeEngine(RealtimeEngine&&) = delete;
    RealtimeEngine& operator=(RealtimeEngine&&) = delete;

    bool prepare(std::size_t maxFrames, std::size_t maxChannels = 8);
    void reset();

    void setParams(const EngineParams& params) noexcept;
    EngineParams paramsSnapshot() const noexcept;

    void processInterleaved(const float* input,
                            float* output,
                            std::size_t frames,
                            std::size_t channels) noexcept;

    RealtimeSnapshot snapshot() const noexcept;

private:
    struct AtomicParams {
        std::atomic<float> footstepEnhance;
        std::atomic<float> actionDetail;
        std::atomic<float> gunshotReduction;
        std::atomic<float> explosionReduction;
        std::atomic<float> detectionSensitivity;
        std::atomic<float> outputCeilingDb;
        std::atomic<float> stepBodyBoostDb;
        std::atomic<float> stepClarityBoostDb;
        std::atomic<float> stepLowBodyBoostDb;
        std::atomic<float> stepLowMidBoostDb;
        std::atomic<float> weaponMidCutDb;
        std::atomic<float> weaponAirCutDb;
        std::atomic<float> sustainedHoldMs;
        std::atomic<float> masterDuckDb;
        std::atomic<float> impactDuckDb;
        std::atomic<float> footstepLevelerAmount;
        std::atomic<float> footstepTargetRmsDb;
        std::atomic<float> footstepMaxLiftDb;
        std::atomic<float> footstepLevelerSpeedMs;
        std::atomic<float> stabilityAmount;
        std::atomic<float> spectralFloorDb;
        std::atomic<float> stableReleaseMs;
        std::atomic<float> footstepGuardAmount;
        std::atomic<float> maxCutStepDb;
        std::atomic<float> transientKill;
        std::atomic<float> lookaheadMs;
        std::atomic<float> outputTrimDb;
        std::atomic<float> residualReductionDb;
        std::atomic<float> balanceLowDb;
        std::atomic<float> balanceMidDb;
        std::atomic<float> balanceHighDb;
        std::atomic<float> stftCutoffHz;
        std::atomic<float> stftPreserveDb;
        std::atomic<float> spectralFloorStab;
        std::atomic<float> protectionPasos;
        std::atomic<float> changeIntensity;
        std::atomic<float> subtletyAmount;
        std::atomic<float> wetMix;
        std::atomic<float> lowShelfFreqHz;
        std::atomic<float> lowMidFreqHz;
        std::atomic<float> lowMidQ;
        std::atomic<float> weaponMidFreqHz;
        std::atomic<float> weaponMidQ;
        std::atomic<float> stepBodyFreqHz;
        std::atomic<float> stepBodyQ;
        std::atomic<float> stepClarityFreqHz;
        std::atomic<float> stepClarityQ;
        std::atomic<float> weaponAirFreqHz;
        std::atomic<float> weaponAirQ;
        std::atomic<float> protectionAttackMs;
        std::atomic<float> protectionReleaseMs;
        std::atomic<float> boostAttackMs;
        std::atomic<float> boostReleaseMs;
        std::atomic<float> limiterReleaseMs;
        std::atomic<float> stereoWidth;
        std::atomic<int> protectionExtreme;
        std::atomic<int> spectralMaskEnabled;
        std::atomic<int> debugLogging;

        AtomicParams() noexcept;
        void store(const EngineParams& params) noexcept;
        EngineParams load() const noexcept;
    };

    void applyPendingParamsIfNeeded() noexcept;
    void publishSnapshot() noexcept;
    void passthrough(const float* input, float* output, std::size_t frames, std::size_t channels) noexcept;

    DspEngine engine_;
    std::vector<float> left_;
    std::vector<float> right_;
    std::vector<float> outLeft_;
    std::vector<float> outRight_;
    std::size_t maxFramesPrepared_ = 0;
    std::size_t maxChannelsPrepared_ = 0;

    AtomicParams pendingParams_;
    EngineParams cachedParams_;
    std::atomic<unsigned long long> pendingRevision_{1};
    unsigned long long appliedRevision_ = 0;
    unsigned long long appliedUpdates_ = 0;

    std::atomic<float> snapshotFootstep_{0.0f};
    std::atomic<float> snapshotAction_{0.0f};
    std::atomic<float> snapshotProtection_{0.0f};
    std::atomic<float> snapshotLateral_{0.0f};
    std::atomic<float> snapshotConfidence_{0.0f};
    std::atomic<float> snapshotOutputPeak_{0.0f};
    std::atomic<unsigned long long> snapshotFramesAnalyzed_{0};
    std::atomic<unsigned long long> blocksProcessed_{0};
    std::atomic<unsigned long long> parameterUpdatesApplied_{0};
    std::atomic<unsigned long long> bypassedBlocks_{0};
    std::atomic<unsigned long long> oversizedBlocks_{0};
    std::atomic<std::size_t> lastFrames_{0};
    std::atomic<std::size_t> lastChannels_{0};
};

} // namespace warzone_audio
