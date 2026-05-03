#include <algorithm>
#include <cstdint>
#include <fstream>
#include <iostream>
#include <limits>
#include <string>
#include <vector>

#include "warzone_audio/Config.h"
#include "warzone_audio/DspEngine.h"
#include "warzone_audio/ScoreLogger.h"

namespace {

struct WavData {
    uint16_t channels = 0;
    uint32_t sampleRate = 0;
    uint16_t bitsPerSample = 0;
    std::vector<float> left;
    std::vector<float> right;
};

uint16_t readU16(std::istream& input)
{
    uint8_t b[2]{};
    input.read(reinterpret_cast<char*>(b), 2);
    return static_cast<uint16_t>(b[0] | (b[1] << 8));
}

uint32_t readU32(std::istream& input)
{
    uint8_t b[4]{};
    input.read(reinterpret_cast<char*>(b), 4);
    return static_cast<uint32_t>(b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24));
}

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

bool readWav(const std::string& path, WavData& wav, std::string& error)
{
    std::ifstream input(path, std::ios::binary);
    if (!input) {
        error = "Could not open input WAV: " + path;
        return false;
    }

    char riff[4]{};
    char wave[4]{};
    input.read(riff, 4);
    (void)readU32(input);
    input.read(wave, 4);
    if (std::string(riff, 4) != "RIFF" || std::string(wave, 4) != "WAVE") {
        error = "Input is not RIFF/WAVE";
        return false;
    }

    uint16_t audioFormat = 0;
    uint16_t channels = 0;
    uint32_t sampleRate = 0;
    uint16_t bitsPerSample = 0;
    std::vector<uint8_t> data;

    while (input) {
        char id[4]{};
        input.read(id, 4);
        if (!input) {
            break;
        }
        const uint32_t size = readU32(input);
        const std::string chunk(id, 4);

        if (chunk == "fmt ") {
            audioFormat = readU16(input);
            channels = readU16(input);
            sampleRate = readU32(input);
            (void)readU32(input);
            (void)readU16(input);
            bitsPerSample = readU16(input);
            if (size > 16) {
                input.seekg(size - 16, std::ios::cur);
            }
        } else if (chunk == "data") {
            data.resize(size);
            input.read(reinterpret_cast<char*>(data.data()), size);
        } else {
            input.seekg(size, std::ios::cur);
        }

        if ((size & 1u) != 0u) {
            input.seekg(1, std::ios::cur);
        }
    }

    if (data.empty() || channels == 0) {
        error = "WAV missing fmt or data chunk";
        return false;
    }
    if (sampleRate != 48000) {
        error = "Only 48 kHz WAV is supported for now";
        return false;
    }
    if (channels != 1 && channels != 2) {
        error = "Only mono/stereo WAV is supported by this offline tool";
        return false;
    }
    if (!((audioFormat == 1 && (bitsPerSample == 16 || bitsPerSample == 24)) ||
          (audioFormat == 3 && bitsPerSample == 32))) {
        error = "Only PCM16, PCM24, or float32 WAV is supported";
        return false;
    }

    wav.channels = channels;
    wav.sampleRate = sampleRate;
    wav.bitsPerSample = bitsPerSample;

    const uint16_t bytesPerSample = static_cast<uint16_t>(bitsPerSample / 8);
    const std::size_t frames = data.size() / (bytesPerSample * channels);
    wav.left.assign(frames, 0.0f);
    wav.right.assign(frames, 0.0f);

    for (std::size_t i = 0; i < frames; ++i) {
        for (uint16_t ch = 0; ch < channels; ++ch) {
            const std::size_t offset = (i * channels + ch) * bytesPerSample;
            float sample = 0.0f;
            if (audioFormat == 1 && bitsPerSample == 16) {
                const auto raw = static_cast<int16_t>(data[offset] | (data[offset + 1] << 8));
                sample = static_cast<float>(raw) / 32768.0f;
            } else if (audioFormat == 1 && bitsPerSample == 24) {
                int32_t raw = static_cast<int32_t>(data[offset]) |
                              (static_cast<int32_t>(data[offset + 1]) << 8) |
                              (static_cast<int32_t>(data[offset + 2]) << 16);
                if ((raw & 0x00800000) != 0) {
                    raw |= static_cast<int32_t>(0xFF000000);
                }
                sample = static_cast<float>(raw) / 8388608.0f;
            } else {
                float raw = 0.0f;
                std::copy(data.begin() + static_cast<std::ptrdiff_t>(offset),
                          data.begin() + static_cast<std::ptrdiff_t>(offset + 4),
                          reinterpret_cast<uint8_t*>(&raw));
                sample = raw;
            }

            if (channels == 1) {
                wav.left[i] = sample;
                wav.right[i] = sample;
            } else if (ch == 0) {
                wav.left[i] = sample;
            } else {
                wav.right[i] = sample;
            }
        }
    }

    return true;
}

bool writeWav16(const std::string& path, const std::vector<float>& left, const std::vector<float>& right, std::string& error)
{
    if (left.size() != right.size()) {
        error = "Channel size mismatch";
        return false;
    }

    std::ofstream output(path, std::ios::binary | std::ios::trunc);
    if (!output) {
        error = "Could not write output WAV: " + path;
        return false;
    }

    constexpr uint16_t channels = 2;
    constexpr uint32_t sampleRate = 48000;
    constexpr uint16_t bitsPerSample = 16;
    constexpr uint16_t blockAlign = channels * bitsPerSample / 8;
    constexpr uint32_t byteRate = sampleRate * blockAlign;
    const uint32_t dataSize = static_cast<uint32_t>(left.size() * blockAlign);

    output.write("RIFF", 4);
    writeU32(output, 36u + dataSize);
    output.write("WAVE", 4);
    output.write("fmt ", 4);
    writeU32(output, 16);
    writeU16(output, 1);
    writeU16(output, channels);
    writeU32(output, sampleRate);
    writeU32(output, byteRate);
    writeU16(output, blockAlign);
    writeU16(output, bitsPerSample);
    output.write("data", 4);
    writeU32(output, dataSize);

    for (std::size_t i = 0; i < left.size(); ++i) {
        const auto writeSample = [&output](float value) {
            const float clipped = std::max(-1.0f, std::min(0.999969f, value));
            const auto raw = static_cast<int16_t>(clipped * 32767.0f);
            writeU16(output, static_cast<uint16_t>(raw));
        };
        writeSample(left[i]);
        writeSample(right[i]);
    }

    return true;
}

void printUsage()
{
    std::cerr << "Usage: wav_process.exe <input.wav> <output.wav> [config.ini] [log.csv]\n";
}

} // namespace

int main(int argc, char** argv)
{
    if (argc < 3) {
        printUsage();
        return 2;
    }

    const std::string inputPath = argv[1];
    const std::string outputPath = argv[2];
    const std::string configPath = argc >= 4 ? argv[3] : "config/default_settings.ini";
    const std::string logPathOverride = argc >= 5 ? argv[4] : "";

    warzone_audio::AppConfig config;
    std::string error;
    if (!warzone_audio::loadConfigFile(configPath, config, &error)) {
        std::cerr << "config_warning=" << error << "\n";
    }
    if (!logPathOverride.empty()) {
        config.logging.logPath = logPathOverride;
        config.engine.debugLogging = true;
    }

    WavData input;
    if (!readWav(inputPath, input, error)) {
        std::cerr << "error=" << error << "\n";
        return 1;
    }

    std::vector<float> outL(input.left.size(), 0.0f);
    std::vector<float> outR(input.right.size(), 0.0f);

    warzone_audio::DspEngine engine;
    engine.setParams(config.engine);

    warzone_audio::ScoreLogger logger;
    if (config.engine.debugLogging) {
        if (!logger.open(config.logging.logPath, config.logging.logEveryFrames)) {
            std::cerr << "log_warning=could not open " << config.logging.logPath << "\n";
        }
    }

    constexpr std::size_t blockSize = 128;
    for (std::size_t offset = 0; offset < input.left.size(); offset += blockSize) {
        const std::size_t count = std::min(blockSize, input.left.size() - offset);
        engine.processBlock(input.left.data() + offset,
                            input.right.data() + offset,
                            outL.data() + offset,
                            outR.data() + offset,
                            count);
        logger.write(engine.stats());
    }

    if (!writeWav16(outputPath, outL, outR, error)) {
        std::cerr << "error=" << error << "\n";
        return 1;
    }

    const auto& stats = engine.stats();
    std::cout << "frames_analyzed=" << stats.framesAnalyzed << "\n";
    std::cout << "last_footstep=" << stats.scores.footstep << "\n";
    std::cout << "last_action=" << stats.scores.action << "\n";
    std::cout << "last_protection=" << stats.scores.protection << "\n";
    std::cout << "output=" << outputPath << "\n";
    if (config.engine.debugLogging) {
        std::cout << "log=" << config.logging.logPath << "\n";
    }
    return 0;
}
