#pragma once

#include "Biquad.h"
#include "warzone_audio/DspTypes.h"

namespace warzone_audio {

class Processor {
public:
    void reset();
    void updateTargets(const DetectorScores& scores, const EngineParams& params);
    void processSample(float inL, float inR, float& outL, float& outR, float& peak);

private:
    struct MaskBank {
        Biquad body;
        Biquad crack;
        Biquad air;
        Biquad stepBody;
        Biquad stepClarity;
    };

    void resetFilters();
    void updateFilters();
    void setBandTarget(float& current, float next, float tolerance);
    float follow(float current, float target, float attackMs, float releaseMs) const;
    float limit(float x) const;

    MaskBank mid_{};
    MaskBank side_{};

    bool weaponOnlyMode_ = false;
    bool filtersDirty_ = true;

    float bodyFreqHz_ = 900.0f;
    float bodyQ_ = 1.15f;
    float crackFreqHz_ = 2400.0f;
    float crackQ_ = 1.35f;
    float airFreqHz_ = 7600.0f;
    float airQ_ = 1.15f;
    float stepBodyFreqHz_ = 1550.0f;
    float stepBodyQ_ = 1.35f;
    float stepClarityFreqHz_ = 3500.0f;
    float stepClarityQ_ = 1.85f;

    float wetMix_ = 1.0f;
    float outputTrimDb_ = 0.0f;
    float ceilingAmp_ = 0.8912509f;
    float stereoWidth_ = 1.0f;

    float detectorGunMask_ = 0.0f;
    float detectorProtectMask_ = 0.0f;
    float suppressionAmount_ = 0.0f;
    float bodyAmount_ = 0.0f;
    float crackAmount_ = 0.0f;
    float airAmount_ = 0.0f;
    float tailAmount_ = 0.0f;
    float sideAmount_ = 0.0f;
    float restoreAmount_ = 1.0f;
    float transientAmount_ = 0.0f;
    float sustainReleaseMs_ = 90.0f;
    float detectorAttackMs_ = 0.05f;
    float detectorReleaseMs_ = 35.0f;
    float guardAmount_ = 0.85f;

    float rmsState_ = 0.0f;
    float dcState_ = 0.0f;
    float bodyEnv_ = 0.0f;
    float crackEnv_ = 0.0f;
    float airEnv_ = 0.0f;
    float stepEnv_ = 0.0f;
    float sideEnv_ = 0.0f;
    float transientEnv_ = 0.0f;
    float gunHold_ = 0.0f;
    float tailStateMid_ = 0.0f;
    float tailStateSide_ = 0.0f;
};

} // namespace warzone_audio
