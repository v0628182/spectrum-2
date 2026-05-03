#pragma once

#include <array>

#include "Biquad.h"
#include "warzone_audio/DspTypes.h"

namespace warzone_audio {

class Processor {
public:
    void reset();
    void updateTargets(const DetectorScores& scores, const EngineParams& params);
    void processSample(float inL, float inR, float& outL, float& outR, float& peak);

private:
    struct Channel {
        Biquad lowShelf;
        Biquad lowMid;
        Biquad weaponMid;
        Biquad stepBody;
        Biquad step;
        Biquad air;
    };

    void updateFilters();
    float limit(float x);
    float slewControl(float current, float target, float maxDownDb, float maxUpDb) const;

    std::array<Channel, 2> channels_{};
    float lowShelfDb_ = 0.0f;
    float lowMidDb_ = 0.0f;
    float weaponMidDb_ = 0.0f;
    float stepBodyDb_ = 0.0f;
    float stepDb_ = 0.0f;
    float airDb_ = 0.0f;
    float masterDuckDb_ = 0.0f;
    float sustainedWeaponState_ = 0.0f;
    float footstepLevelerDb_ = 0.0f;
    float rmsState_ = 0.0f;
    float limiterGain_ = 1.0f;
    float ceilingAmp_ = 0.8912509f;
};

} // namespace warzone_audio
