#pragma once

#include "warzone_audio/DspTypes.h"

namespace warzone_audio {

class TransientDetector {
public:
    void reset();
    DetectorScores update(const FeatureFrame& frame, const EngineParams& params);

private:
    float footstepState_ = 0.0f;
    float actionState_ = 0.0f;
    float protectionState_ = 0.0f;
};

} // namespace warzone_audio
