#pragma once

#include <array>

#include "Biquad.h"
#include "SpatialTypes.h"
#include "warzone_audio/DspTypes.h"

namespace warzone_audio {

struct SelfWeaponSnapshot {
    float weaponMask = 0.0f;
    float protectMask = 0.0f;
    float centerConfidence = 0.0f;
    float outputPeak = 0.0f;
};

class SelfWeaponSuppressor {
public:
    void reset();
    void updateTargets(const DetectorScores& scores, const EngineParams& params);
    void processFrame(const float* input,
                      float* output,
                      std::size_t channels,
                      const SpatialLayout& layout,
                      float wetMix,
                      float& peak);

    const SelfWeaponSnapshot& snapshot() const { return snapshot_; }

private:
    struct BandBank {
        Biquad body;
        Biquad bodyWide;
        Biquad crack;
        Biquad crackWide;
        Biquad air;
        Biquad airWide;
        Biquad stepBody;
        Biquad stepClarity;

        void reset();
    };

    struct FrontFrame {
        float left = 0.0f;
        float right = 0.0f;
        float center = 0.0f;
        float frontMid = 0.0f;
        float frontSide = 0.0f;
        float centerBus = 0.0f;
        float sideBackEnergy = 0.0f;
        bool hasLeftRight = false;
        bool hasCenter = false;
    };

    struct BandFrame {
        float body = 0.0f;
        float bodyWide = 0.0f;
        float crack = 0.0f;
        float crackWide = 0.0f;
        float air = 0.0f;
        float airWide = 0.0f;
        float stepBody = 0.0f;
        float stepClarity = 0.0f;
        float transient = 0.0f;
    };

    void resetFilters();
    void updateFilters();
    void setBandTarget(float& current, float next, float tolerance);
    void configureBand(Biquad& filter, float frequencyHz, float q);

    FrontFrame readFrontFrame(const float* frame, std::size_t channels, const SpatialLayout& layout) const;
    BandFrame processBands(float centerBus);
    void writeFrontFrame(const float* input,
                         float* output,
                         std::size_t channels,
                         const SpatialLayout& layout,
                         const FrontFrame& front,
                         float processedMid,
                         float processedCenter,
                         float lfeScale,
                         float wetMix) const;

    float follow(float current, float target, float attackMs, float releaseMs) const;
    float componentDrive(float value) const;
    float normalizedAmount(float value) const;
    float limit(float x) const;

    BandBank bands_{};
    bool filtersDirty_ = true;
    bool weaponOnlyMode_ = false;

    float bodyFreqHz_ = 900.0f;
    float bodyQ_ = 1.15f;
    float crackFreqHz_ = 2450.0f;
    float crackQ_ = 1.35f;
    float airFreqHz_ = 7600.0f;
    float airQ_ = 1.15f;
    float stepBodyFreqHz_ = 1550.0f;
    float stepBodyQ_ = 1.35f;
    float stepClarityFreqHz_ = 3500.0f;
    float stepClarityQ_ = 1.85f;

    float outputTrimDb_ = 0.0f;
    float ceilingAmp_ = 0.8912509f;
    float detectorAttackMs_ = 0.02f;
    float detectorReleaseMs_ = 16.0f;
    float holdReleaseMs_ = 36.0f;

    float suppressionAmount_ = 0.0f;
    float bodyAmount_ = 0.0f;
    float crackAmount_ = 0.0f;
    float airAmount_ = 0.0f;
    float tailAmount_ = 0.0f;
    float sideAmount_ = 0.0f;
    float restoreAmount_ = 1.0f;
    float transientAmount_ = 0.0f;
    float guardAmount_ = 0.85f;
    float spectralFloorAmp_ = 0.0f;
    float maxCutDb_ = 96.0f;

    float detectorGunMask_ = 0.0f;
    float detectorProtectMask_ = 0.0f;
    float activeMask_ = 0.0f;
    float holdMask_ = 0.0f;
    float rmsState_ = 0.0f;
    float sideBackState_ = 0.0f;
    float bodyEnv_ = 0.0f;
    float crackEnv_ = 0.0f;
    float airEnv_ = 0.0f;
    float stepEnv_ = 0.0f;
    float transientEnv_ = 0.0f;
    float dcState_ = 0.0f;
    float tailState_ = 0.0f;
    float repeatState_ = 0.0f;
    float sampleCounter_ = 0.0f;
    float lastImpulseSample_ = -48000.0f;

    SelfWeaponSnapshot snapshot_{};
};

} // namespace warzone_audio
