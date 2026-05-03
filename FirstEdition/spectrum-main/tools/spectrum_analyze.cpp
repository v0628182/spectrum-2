#include <algorithm>
#include <cmath>
#include <cstdint>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

#include "Fft.h"
#include "warzone_audio/Constants.h"

namespace {

using warzone_audio::constants::kBinHz;
using warzone_audio::constants::kEpsEnergy;
using warzone_audio::constants::kFftSize;
using warzone_audio::constants::kHopSize;
using warzone_audio::constants::kPi;
using warzone_audio::constants::kPositiveBins;

struct WavData {
    uint16_t channels = 0;
    uint32_t sampleRate = 0;
    uint16_t bitsPerSample = 0;
    std::vector<float> left;
    std::vector<float> right;
};

struct Band {
    const char* name;
    float lo;
    float hi;
    double sumPower = 0.0;
    double maxPower = 0.0;
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

bool readWav(const std::string& path, WavData& wav, std::string& error)
{
    std::ifstream input(path, std::ios::binary);
    if (!input) {
        error = "Could not open WAV";
        return false;
    }

    char riff[4]{};
    char wave[4]{};
    input.read(riff, 4);
    (void)readU32(input);
    input.read(wave, 4);
    if (std::string(riff, 4) != "RIFF" || std::string(wave, 4) != "WAVE") {
        error = "Not RIFF/WAVE";
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

    if (audioFormat != 1 || !(bitsPerSample == 16 || bitsPerSample == 24) || sampleRate != 48000 ||
        (channels != 1 && channels != 2)) {
        error = "Only 48 kHz PCM16/PCM24 mono/stereo supported";
        return false;
    }

    const uint16_t bytesPerSample = static_cast<uint16_t>(bitsPerSample / 8);
    const std::size_t frames = data.size() / (bytesPerSample * channels);
    wav.channels = channels;
    wav.sampleRate = sampleRate;
    wav.bitsPerSample = bitsPerSample;
    wav.left.assign(frames, 0.0f);
    wav.right.assign(frames, 0.0f);

    for (std::size_t i = 0; i < frames; ++i) {
        for (uint16_t ch = 0; ch < channels; ++ch) {
            const std::size_t offset = (i * channels + ch) * bytesPerSample;
            float sample = 0.0f;
            if (bitsPerSample == 16) {
                const auto raw = static_cast<int16_t>(data[offset] | (data[offset + 1] << 8));
                sample = static_cast<float>(raw) / 32768.0f;
            } else {
                int32_t raw = static_cast<int32_t>(data[offset]) |
                              (static_cast<int32_t>(data[offset + 1]) << 8) |
                              (static_cast<int32_t>(data[offset + 2]) << 16);
                if ((raw & 0x00800000) != 0) {
                    raw |= static_cast<int32_t>(0xFF000000);
                }
                sample = static_cast<float>(raw) / 8388608.0f;
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

std::size_t binMin(float hz)
{
    return static_cast<std::size_t>(std::ceil(hz / kBinHz));
}

std::size_t binMax(float hz)
{
    return std::min<std::size_t>(kPositiveBins - 1, static_cast<std::size_t>(std::floor(hz / kBinHz)));
}

double toDb(double power)
{
    return 10.0 * std::log10(std::max(power, static_cast<double>(kEpsEnergy)));
}

} // namespace

int main(int argc, char** argv)
{
    if (argc < 2) {
        std::cerr << "Usage: spectrum_analyze.exe <input.wav>\n";
        return 2;
    }

    WavData wav;
    std::string error;
    if (!readWav(argv[1], wav, error)) {
        std::cerr << "error=" << error << "\n";
        return 1;
    }

    std::vector<Band> bands = {
        {"bass_80_250", 80.0f, 250.0f},
        {"lowmid_250_700", 250.0f, 700.0f},
        {"body_700_1500", 700.0f, 1500.0f},
        {"presence_1500_2500", 1500.0f, 2500.0f},
        {"step_low_1800_2800", 1800.0f, 2800.0f},
        {"step_core_2500_5000", 2500.0f, 5000.0f},
        {"step_high_4000_6500", 4000.0f, 6500.0f},
        {"air_6500_9000", 6500.0f, 9000.0f},
        {"noise_9000_12000", 9000.0f, 12000.0f},
    };

    std::vector<double> avgPower(kPositiveBins, 0.0);
    std::vector<double> maxPower(kPositiveBins, 0.0);
    std::vector<std::complex<float>> fftBuffer(kFftSize);
    std::vector<float> window(kFftSize);
    for (std::size_t n = 0; n < kFftSize; ++n) {
        window[n] = 0.5f - 0.5f * std::cos(2.0f * kPi * static_cast<float>(n) / static_cast<float>(kFftSize));
    }

    warzone_audio::Fft fft(kFftSize);
    std::size_t frames = 0;
    double centroidSum = 0.0;
    double centroidWeight = 0.0;

    for (std::size_t start = 0; start + kFftSize <= wav.left.size(); start += kHopSize) {
        double rms = 0.0;
        for (std::size_t n = 0; n < kFftSize; ++n) {
            const float mid = 0.5f * (wav.left[start + n] + wav.right[start + n]);
            rms += static_cast<double>(mid) * static_cast<double>(mid);
            fftBuffer[n] = std::complex<float>(mid * window[n], 0.0f);
        }
        rms = std::sqrt(rms / static_cast<double>(kFftSize));
        if (rms < 0.003) {
            continue;
        }

        fft.forward(fftBuffer);
        ++frames;

        double totalPower = 0.0;
        double weightedHz = 0.0;
        for (std::size_t k = 1; k < kPositiveBins; ++k) {
            const double p = std::norm(fftBuffer[k]);
            avgPower[k] += p;
            maxPower[k] = std::max(maxPower[k], p);
            totalPower += p;
            weightedHz += p * static_cast<double>(k) * kBinHz;
        }
        centroidSum += weightedHz;
        centroidWeight += totalPower;

        for (auto& band : bands) {
            double sum = 0.0;
            for (std::size_t k = binMin(band.lo); k <= binMax(band.hi); ++k) {
                sum += std::norm(fftBuffer[k]);
            }
            band.sumPower += sum;
            band.maxPower = std::max(band.maxPower, sum);
        }
    }

    if (frames == 0) {
        std::cerr << "error=no active frames\n";
        return 1;
    }

    std::cout << "file=" << argv[1] << "\n";
    std::cout << "channels=" << wav.channels << "\n";
    std::cout << "bits=" << wav.bitsPerSample << "\n";
    std::cout << "active_frames=" << frames << "\n";
    std::cout << "centroid_hz=" << (centroidSum / std::max(centroidWeight, 1.0)) << "\n\n";

    std::cout << "bands_avg_db\n";
    for (const auto& band : bands) {
        std::cout << band.name << '=' << toDb(band.sumPower / static_cast<double>(frames)) << "\n";
    }

    std::cout << "\ntop_bins_avg\n";
    std::vector<std::size_t> bins;
    for (std::size_t k = 1; k <= binMax(12000.0f); ++k) {
        bins.push_back(k);
    }
    std::sort(bins.begin(), bins.end(), [&](std::size_t a, std::size_t b) {
        return avgPower[a] > avgPower[b];
    });
    for (std::size_t i = 0; i < std::min<std::size_t>(20, bins.size()); ++i) {
        const std::size_t k = bins[i];
        std::cout << static_cast<double>(k) * kBinHz << "Hz=" << toDb(avgPower[k] / static_cast<double>(frames)) << "\n";
    }

    std::cout << "\ntop_bins_peak\n";
    std::sort(bins.begin(), bins.end(), [&](std::size_t a, std::size_t b) {
        return maxPower[a] > maxPower[b];
    });
    for (std::size_t i = 0; i < std::min<std::size_t>(20, bins.size()); ++i) {
        const std::size_t k = bins[i];
        std::cout << static_cast<double>(k) * kBinHz << "Hz=" << toDb(maxPower[k]) << "\n";
    }

    return 0;
}
