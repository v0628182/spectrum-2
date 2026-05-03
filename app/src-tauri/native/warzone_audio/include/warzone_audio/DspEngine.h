#pragma once

#include <array>
#include <cstddef>
#include <memory>
#include <vector>

#include "warzone_audio/DspTypes.h"

namespace warzone_audio {

class DspEngine {
public:
    DspEngine();
    ~DspEngine();

    DspEngine(const DspEngine&) = delete;
    DspEngine& operator=(const DspEngine&) = delete;
    DspEngine(DspEngine&&) noexcept;
    DspEngine& operator=(DspEngine&&) noexcept;

    void reset();
    void setParams(const EngineParams& params);
    const EngineParams& params() const;

    void processBlock(const float* inL, const float* inR, float* outL, float* outR, std::size_t numSamples);
    const ProcessStats& stats() const;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

} // namespace warzone_audio
