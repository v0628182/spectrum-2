#pragma once

#include <array>
#include <complex>
#include <vector>

#include "Fft.h"
#include "warzone_audio/DspTypes.h"

namespace warzone_audio {

class FeatureExtractor {
public:
    FeatureExtractor();

    void reset();
    FeatureFrame analyze(const std::array<float, constants::kFftSize>& left,
                         const std::array<float, constants::kFftSize>& right);

private:
    float bandEnergyDb(const std::array<float, constants::kPositiveBins>& power, BinRange range) const;
    float bandSuperFlux(const std::array<float, constants::kPositiveBins>& logMag, BinRange range) const;
    float updateNoise(float currentNoiseDb, float energyDb) const;

    Fft fft_;
    std::array<float, constants::kFftSize> window_{};
    std::vector<std::complex<float>> fftBuffer_;
    std::array<float, constants::kPositiveBins> prevLogMag_{};
    std::array<float, constants::kPositiveBins> magMid_{};
    std::array<float, constants::kPositiveBins> logMagMid_{};
    std::array<float, constants::kPositiveBins> powMid_{};
    BandEnergiesDb noiseDb_{};
    BandEnergiesDb slowEnergyDb_{};
    float fluxNoiseStep_ = 0.0f;
    float fluxNoisePresence_ = 0.0f;
    float fluxNoiseBroadband_ = 0.0f;
    float activeStepFrames_ = 0.0f;
    unsigned warmupFrames_ = 0;
    bool hasPrevious_ = false;
};

} // namespace warzone_audio
