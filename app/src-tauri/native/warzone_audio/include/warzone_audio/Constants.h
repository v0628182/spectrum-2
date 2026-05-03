#pragma once

#include <array>
#include <cstddef>

namespace warzone_audio {

struct BinRange {
    std::size_t first;
    std::size_t last;
};

namespace constants {

constexpr float kSampleRate = 48000.0f;
constexpr std::size_t kFftSize = 512;
constexpr std::size_t kHopSize = 128;
constexpr std::size_t kPositiveBins = kFftSize / 2 + 1;
constexpr float kBinHz = kSampleRate / static_cast<float>(kFftSize);
constexpr float kHopMs = 1000.0f * static_cast<float>(kHopSize) / kSampleRate;

constexpr float kPi = 3.14159265358979323846f;
constexpr float kEpsAmp = 1.0e-12f;
constexpr float kEpsEnergy = 1.0e-20f;

constexpr BinRange kBassBins{1, 3};
constexpr BinRange kLowMidBins{4, 10};
constexpr BinRange kMidBins{11, 26};
constexpr BinRange kStepBins{27, 53};
constexpr BinRange kAirBins{54, 85};
constexpr BinRange kNoiseBins{86, 128};

constexpr float kTauFastMs = 5.0f;
constexpr float kTauMedMs = 25.0f;
constexpr float kTauSlowMs = 250.0f;
constexpr float kTauNoiseMs = 1000.0f;

constexpr float kStepFluxMin = 0.08f;
constexpr float kStepFluxStrong = 0.20f;
constexpr float kActionFluxMin = 0.06f;
constexpr float kProtectionFluxMin = 0.12f;

constexpr int kSuperFluxRadiusBins = 1;
constexpr float kSuperFluxStepMin = 0.06f;
constexpr float kSuperFluxStepStrong = 0.16f;

constexpr float kCentroidStepMinHz = 1800.0f;
constexpr float kCentroidStepIdealHz = 3500.0f;

constexpr float kFlatnessStepTarget = 0.45f;
constexpr float kCrestTransientMinDb = 8.0f;
constexpr float kCrestStepMinDb = 6.0f;
constexpr float kCrestProtectionMinDb = 10.0f;

constexpr float kStepAttackMinDb = 5.0f;
constexpr float kStepAttackStrongDb = 10.0f;
constexpr float kActionAttackMinDb = 4.0f;
constexpr float kProtectionAttackMinDb = 12.0f;

constexpr float kStepMinDurationMs = 8.0f;
constexpr float kStepMaxDurationMs = 120.0f;
constexpr float kActionMaxDurationMs = 180.0f;

constexpr float kFootstepTrigger = 0.66f;
constexpr float kFootstepStrong = 0.82f;
constexpr float kActionTrigger = 0.58f;
constexpr float kProtectionTrigger = 0.48f;
constexpr float kProtectionExtreme = 0.72f;

constexpr float kFootstepBoostDbMax = 18.0f;
constexpr float kActionBoostDbMax = 3.0f;
constexpr float kBassReductionDbMax = -36.0f;
constexpr float kLowMidReductionDbMax = -24.0f;
constexpr float kMidReductionDbMax = -9.0f;
constexpr float kStepReductionDuringProtectionDb = -2.0f;

constexpr float kBoostAttackMs = 3.0f;
constexpr float kBoostReleaseMs = 90.0f;
constexpr float kProtectionAttackMs = 1.0f;
constexpr float kProtectionReleaseMs = 140.0f;
constexpr float kOutputCeilingDb = -1.0f;

} // namespace constants
} // namespace warzone_audio
