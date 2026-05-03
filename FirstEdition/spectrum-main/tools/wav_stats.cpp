#include <algorithm>
#include <cstdint>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

namespace {

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

bool readSamples(const std::string& path, std::vector<float>& samples, uint32_t& sampleRate, uint16_t& channels)
{
    std::ifstream input(path, std::ios::binary);
    if (!input) {
        return false;
    }

    char riff[4]{};
    char wave[4]{};
    input.read(riff, 4);
    (void)readU32(input);
    input.read(wave, 4);

    uint16_t audioFormat = 0;
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
        return false;
    }

    const uint16_t bytesPerSample = static_cast<uint16_t>(bitsPerSample / 8);
    const std::size_t count = data.size() / bytesPerSample;
    samples.assign(count, 0.0f);

    for (std::size_t i = 0; i < count; ++i) {
        const std::size_t offset = i * bytesPerSample;
        if (audioFormat == 1 && bitsPerSample == 16) {
            const auto raw = static_cast<int16_t>(data[offset] | (data[offset + 1] << 8));
            samples[i] = static_cast<float>(raw) / 32768.0f;
        } else if (audioFormat == 1 && bitsPerSample == 24) {
            int32_t raw = static_cast<int32_t>(data[offset]) |
                          (static_cast<int32_t>(data[offset + 1]) << 8) |
                          (static_cast<int32_t>(data[offset + 2]) << 16);
            if ((raw & 0x00800000) != 0) {
                raw |= static_cast<int32_t>(0xFF000000);
            }
            samples[i] = static_cast<float>(raw) / 8388608.0f;
        } else {
            return false;
        }
    }
    return true;
}

} // namespace

int main(int argc, char** argv)
{
    if (argc < 2) {
        std::cerr << "Usage: wav_stats.exe <file.wav>\n";
        return 2;
    }

    std::vector<float> samples;
    uint32_t sampleRate = 0;
    uint16_t channels = 0;
    if (!readSamples(argv[1], samples, sampleRate, channels)) {
        std::cerr << "error=unsupported wav\n";
        return 1;
    }

    double sumSq = 0.0;
    float peak = 0.0f;
    for (float sample : samples) {
        peak = std::max(peak, std::abs(sample));
        sumSq += static_cast<double>(sample) * static_cast<double>(sample);
    }

    const double rms = std::sqrt(sumSq / static_cast<double>(samples.size()));
    std::cout << "path=" << argv[1] << "\n";
    std::cout << "sample_rate=" << sampleRate << "\n";
    std::cout << "channels=" << channels << "\n";
    std::cout << "peak=" << peak << "\n";
    std::cout << "rms=" << rms << "\n";
    return 0;
}
