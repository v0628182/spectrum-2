#pragma once

#include <algorithm>
#include <cmath>

#include "warzone_audio/Constants.h"

namespace warzone_audio {

inline float clamp(float value, float lo, float hi)
{
    return std::max(lo, std::min(value, hi));
}

inline float saturate(float value)
{
    return clamp(value, 0.0f, 1.0f);
}

inline float ramp(float value, float lo, float hi)
{
    if (hi <= lo) {
        return value >= hi ? 1.0f : 0.0f;
    }
    return saturate((value - lo) / (hi - lo));
}

inline float dbToAmp(float db)
{
    return std::pow(10.0f, db / 20.0f);
}

inline float ampToDb(float amp)
{
    return 20.0f * std::log10(std::max(std::abs(amp), constants::kEpsAmp));
}

inline float powerToDb(float power)
{
    return 10.0f * std::log10(std::max(power, constants::kEpsEnergy));
}

inline float onePoleAlpha(float tauMs)
{
    return std::exp(-constants::kHopMs / std::max(tauMs, 0.001f));
}

inline float smooth(float previous, float next, float tauMs)
{
    const float alpha = onePoleAlpha(tauMs);
    return alpha * previous + (1.0f - alpha) * next;
}

inline float approachDb(float current, float target, float attackMs, float releaseMs)
{
    const float tau = target > current ? attackMs : releaseMs;
    return smooth(current, target, tau);
}

} // namespace warzone_audio
