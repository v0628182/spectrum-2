#include <cmath>
#include <cstdint>
#include <fstream>
#include <iostream>
#include <string>

#include "warzone_audio/Constants.h"

namespace {

void writeU16(std::ostream& output, uint16_t value)
{
    const uint8_t b[2] = {
        static_cast<uint8_t>(value & 0xFF),
        static_cast<uint8_t>((value >> 8) & 0xFF),
    };
    output.write(reinterpret_cast<const char*>(b), 2);
}

void writeU32(std::ostream& output, uint32_t value)
{
    const uint8_t b[4] = {
        static_cast<uint8_t>(value & 0xFF),
        static_cast<uint8_t>((value >> 8) & 0xFF),
        static_cast<uint8_t>((value >> 16) & 0xFF),
        static_cast<uint8_t>((value >> 24) & 0xFF),
    };
    output.write(reinterpret_cast<const char*>(b), 4);
}

float sample(float t, float channelPan)
{
    float value = 0.01f * std::sin(2.0f * warzone_audio::constants::kPi * 220.0f * t);
    if (t > 0.35f && t < 0.375f) {
        const float local = (t - 0.35f) / 0.025f;
        const float env = std::sin(warzone_audio::constants::kPi * local);
        value += channelPan * 0.18f * env * std::sin(2.0f * warzone_audio::constants::kPi * 3500.0f * t);
    }
    if (t > 0.70f && t < 0.86f) {
        const float local = (t - 0.70f) / 0.16f;
        const float env = std::exp(-5.0f * local);
        value += 0.65f * env * std::sin(2.0f * warzone_audio::constants::kPi * 120.0f * t);
    }
    return value;
}

} // namespace

int main(int argc, char** argv)
{
    const std::string path = argc >= 2 ? argv[1] : "build/fixture_input.wav";
    std::ofstream output(path, std::ios::binary | std::ios::trunc);
    if (!output) {
        std::cerr << "Could not write " << path << "\n";
        return 1;
    }

    constexpr uint32_t sampleRate = 48000;
    constexpr uint16_t channels = 2;
    constexpr uint16_t bitsPerSample = 16;
    constexpr uint16_t blockAlign = channels * bitsPerSample / 8;
    constexpr uint32_t seconds = 1;
    constexpr uint32_t frames = sampleRate * seconds;
    constexpr uint32_t dataSize = frames * blockAlign;

    output.write("RIFF", 4);
    writeU32(output, 36u + dataSize);
    output.write("WAVE", 4);
    output.write("fmt ", 4);
    writeU32(output, 16);
    writeU16(output, 1);
    writeU16(output, channels);
    writeU32(output, sampleRate);
    writeU32(output, sampleRate * blockAlign);
    writeU16(output, blockAlign);
    writeU16(output, bitsPerSample);
    output.write("data", 4);
    writeU32(output, dataSize);

    for (uint32_t i = 0; i < frames; ++i) {
        const float t = static_cast<float>(i) / static_cast<float>(sampleRate);
        const float l = std::max(-1.0f, std::min(0.999969f, sample(t, 0.75f)));
        const float r = std::max(-1.0f, std::min(0.999969f, sample(t, 1.0f)));
        writeU16(output, static_cast<uint16_t>(static_cast<int16_t>(l * 32767.0f)));
        writeU16(output, static_cast<uint16_t>(static_cast<int16_t>(r * 32767.0f)));
    }

    std::cout << path << "\n";
    return 0;
}
