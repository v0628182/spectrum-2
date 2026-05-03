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
        Biquad weaponBody;
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
    float weaponBodyDb_ = 0.0f;
    float weaponMidDb_ = 0.0f;
    float stepBodyDb_ = 0.0f;
    float stepDb_ = 0.0f;
    float airDb_ = 0.0f;
    float masterDuckDb_ = 0.0f;
    float outputTrimDb_ = 0.0f;
    float maskCutoffHz_ = 2500.0f;
    float lowShelfFreqHz_ = 250.0f;
    float lowMidFreqHz_ = 650.0f;
    float lowMidQ_ = 0.90f;
    float weaponBodyFreqHz_ = 900.0f;
    float weaponBodyQ_ = 1.20f;
    float weaponMidFreqHz_ = 1600.0f;
    float weaponMidQ_ = 0.85f;
    float stepBodyFreqHz_ = 1550.0f;
    float stepBodyQ_ = 1.35f;
    float stepClarityFreqHz_ = 3500.0f;
    float stepClarityQ_ = 1.85f;
    float weaponAirFreqHz_ = 6500.0f;
    float weaponAirQ_ = 1.00f;
    float wetMix_ = 1.0f;
    float limiterReleaseMs_ = 0.5f;
    float stereoWidth_ = 1.0f;
    bool weaponOnlyMode_ = false;
    float transientGate_ = 0.0f;
    float transientState_ = 0.0f;
    float sustainedWeaponState_ = 0.0f;
    float footstepLevelerDb_ = 0.0f;
    float rmsState_ = 0.0f;
    float limiterGain_ = 1.0f;
    float ceilingAmp_ = 0.8912509f;
};

} // namespace warzone_audio
