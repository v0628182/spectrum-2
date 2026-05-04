#pragma once

#include <cstddef>
#include <cstdint>
#include <memory>

#include "warzone_audio/DspTypes.h"

namespace warzone_audio {

class SpatialDspEngine {
public:
    SpatialDspEngine();
    ~SpatialDspEngine();

    SpatialDspEngine(const SpatialDspEngine&) = delete;
    SpatialDspEngine& operator=(const SpatialDspEngine&) = delete;
    SpatialDspEngine(SpatialDspEngine&&) noexcept;
    SpatialDspEngine& operator=(SpatialDspEngine&&) noexcept;

    void reset();
    void setParams(const EngineParams& params);
    const EngineParams& params() const;

    void processInterleaved(const float* input,
                            float* output,
                            std::size_t frames,
                            std::size_t channels,
                            std::uint32_t channelMask);

    const ProcessStats& stats() const;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

} // namespace warzone_audio
