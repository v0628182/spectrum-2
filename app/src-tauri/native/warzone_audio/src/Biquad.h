#pragma once

namespace warzone_audio {

class Biquad {
public:
    void reset();
    void setPeaking(float sampleRate, float frequencyHz, float q, float gainDb);
    void setLowShelf(float sampleRate, float frequencyHz, float q, float gainDb);
    float process(float x);

private:
    void setNormalized(float b0, float b1, float b2, float a0, float a1, float a2);

    float b0_ = 1.0f;
    float b1_ = 0.0f;
    float b2_ = 0.0f;
    float a1_ = 0.0f;
    float a2_ = 0.0f;
    float z1_ = 0.0f;
    float z2_ = 0.0f;
};

} // namespace warzone_audio
