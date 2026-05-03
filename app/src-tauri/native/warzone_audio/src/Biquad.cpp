#include "Biquad.h"

#include <cmath>

#include "warzone_audio/Constants.h"

namespace warzone_audio {

void Biquad::reset()
{
    z1_ = 0.0f;
    z2_ = 0.0f;
}

void Biquad::setNormalized(float b0, float b1, float b2, float a0, float a1, float a2)
{
    const float invA0 = 1.0f / a0;
    b0_ = b0 * invA0;
    b1_ = b1 * invA0;
    b2_ = b2 * invA0;
    a1_ = a1 * invA0;
    a2_ = a2 * invA0;
}

void Biquad::setPeaking(float sampleRate, float frequencyHz, float q, float gainDb)
{
    const float a = std::pow(10.0f, gainDb / 40.0f);
    const float w0 = 2.0f * constants::kPi * frequencyHz / sampleRate;
    const float alpha = std::sin(w0) / (2.0f * q);
    const float cosW0 = std::cos(w0);

    setNormalized(
        1.0f + alpha * a,
        -2.0f * cosW0,
        1.0f - alpha * a,
        1.0f + alpha / a,
        -2.0f * cosW0,
        1.0f - alpha / a);
}

void Biquad::setLowShelf(float sampleRate, float frequencyHz, float q, float gainDb)
{
    const float a = std::pow(10.0f, gainDb / 40.0f);
    const float w0 = 2.0f * constants::kPi * frequencyHz / sampleRate;
    const float sinW0 = std::sin(w0);
    const float cosW0 = std::cos(w0);
    const float beta = std::sqrt(a) / q;

    setNormalized(
        a * ((a + 1.0f) - (a - 1.0f) * cosW0 + beta * sinW0),
        2.0f * a * ((a - 1.0f) - (a + 1.0f) * cosW0),
        a * ((a + 1.0f) - (a - 1.0f) * cosW0 - beta * sinW0),
        (a + 1.0f) + (a - 1.0f) * cosW0 + beta * sinW0,
        -2.0f * ((a - 1.0f) + (a + 1.0f) * cosW0),
        (a + 1.0f) + (a - 1.0f) * cosW0 - beta * sinW0);
}

float Biquad::process(float x)
{
    const float y = b0_ * x + z1_;
    z1_ = b1_ * x - a1_ * y + z2_;
    z2_ = b2_ * x - a2_ * y;
    return y;
}

} // namespace warzone_audio
