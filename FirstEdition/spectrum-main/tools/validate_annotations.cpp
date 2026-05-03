#include <algorithm>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <regex>
#include <sstream>
#include <string>
#include <vector>

#include "warzone_audio/Config.h"
#include "warzone_audio/Constants.h"
#include "warzone_audio/DspEngine.h"

namespace {

struct WavData {
    uint16_t channels = 0;
    uint32_t sampleRate = 0;
    uint16_t bitsPerSample = 0;
    std::vector<float> left;
    std::vector<float> right;
};

struct Marker {
    std::string kind;
    float timeSeconds = 0.0f;
};

struct Annotation {
    std::string clipPath;
    std::string presetPath;
    std::vector<Marker> markers;
};

struct ScorePoint {
    float timeSeconds = 0.0f;
    warzone_audio::ProcessStats stats;
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

std::string readTextFile(const std::string& path)
{
    std::ifstream input(path);
    std::ostringstream text;
    text << input.rdbuf();
    return text.str();
}

std::string jsonEscape(const std::string& value)
{
    std::string escaped;
    for (const char ch : value) {
        if (ch == '\\' || ch == '"') {
            escaped.push_back('\\');
        }
        escaped.push_back(ch);
    }
    return escaped;
}

std::string extractObject(const std::string& json, const std::string& key)
{
    const std::string needle = "\"" + key + "\"";
    const std::size_t keyPos = json.find(needle);
    if (keyPos == std::string::npos) {
        return {};
    }
    const std::size_t open = json.find('{', keyPos);
    if (open == std::string::npos) {
        return {};
    }
    int depth = 0;
    for (std::size_t i = open; i < json.size(); ++i) {
        if (json[i] == '{') {
            ++depth;
        } else if (json[i] == '}') {
            --depth;
            if (depth == 0) {
                return json.substr(open, i - open + 1);
            }
        }
    }
    return {};
}

std::string extractString(const std::string& json, const std::string& key)
{
    const std::regex pattern("\"" + key + "\"\\s*:\\s*\"([^\"]*)\"");
    std::smatch match;
    return std::regex_search(json, match, pattern) ? match[1].str() : "";
}

bool parseAnnotation(const std::string& path, Annotation& annotation, std::string& error)
{
    const std::string json = readTextFile(path);
    if (json.empty()) {
        error = "Could not read annotation JSON";
        return false;
    }

    annotation.clipPath = extractString(extractObject(json, "clip"), "path");
    annotation.presetPath = extractString(extractObject(json, "preset"), "path");
    if (annotation.clipPath.empty()) {
        error = "Annotation missing clip.path";
        return false;
    }

    const std::regex markerPattern(
        "\\{[^{}]*\"kind\"\\s*:\\s*\"([^\"]+)\"[^{}]*\"timeSeconds\"\\s*:\\s*([-+0-9.eE]+)[^{}]*\\}");
    auto begin = std::sregex_iterator(json.begin(), json.end(), markerPattern);
    auto end = std::sregex_iterator();
    for (auto it = begin; it != end; ++it) {
        Marker marker;
        marker.kind = (*it)[1].str();
        marker.timeSeconds = std::stof((*it)[2].str());
        annotation.markers.push_back(marker);
    }

    if (annotation.markers.empty()) {
        error = "Annotation has no markers";
        return false;
    }
    return true;
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
        error = "Only 48 kHz WAV is supported";
        return false;
    }
    if (channels != 1 && channels != 2) {
        error = "Only mono/stereo WAV is supported by validate_annotations";
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

bool isFootstep(const std::string& kind)
{
    return kind == "footstep" || kind == "pasos";
}

bool isProtectionEvent(const std::string& kind)
{
    return kind == "gunshot" || kind == "disparos" || kind == "airstrike";
}

std::vector<ScorePoint> processScores(const WavData& wav, const warzone_audio::EngineParams& params)
{
    warzone_audio::DspEngine engine;
    engine.setParams(params);
    std::vector<ScorePoint> scores;
    std::vector<float> outL(128, 0.0f);
    std::vector<float> outR(128, 0.0f);

    constexpr std::size_t blockSize = 128;
    std::size_t lastFrame = 0;
    for (std::size_t offset = 0; offset < wav.left.size(); offset += blockSize) {
        const std::size_t count = std::min(blockSize, wav.left.size() - offset);
        engine.processBlock(wav.left.data() + offset, wav.right.data() + offset, outL.data(), outR.data(), count);
        const auto& stats = engine.stats();
        if (stats.framesAnalyzed != lastFrame) {
            ScorePoint point;
            point.timeSeconds = static_cast<float>(stats.framesAnalyzed * warzone_audio::constants::kHopSize) /
                                warzone_audio::constants::kSampleRate;
            point.stats = stats;
            scores.push_back(point);
            lastFrame = stats.framesAnalyzed;
        }
    }
    return scores;
}

void printUsage()
{
    std::cerr << "Usage: validate_annotations.exe <markers.json> [config.ini] [report.json]\n";
}

} // namespace

int main(int argc, char** argv)
{
    if (argc < 2) {
        printUsage();
        return 2;
    }

    const std::string annotationPath = argv[1];
    Annotation annotation;
    std::string error;
    if (!parseAnnotation(annotationPath, annotation, error)) {
        std::cerr << "error=" << error << "\n";
        return 1;
    }

    const std::string configPath = argc >= 3 ? argv[2] : annotation.presetPath;
    if (configPath.empty()) {
        std::cerr << "error=No config path supplied and annotation has no preset.path\n";
        return 1;
    }

    std::filesystem::path reportPath;
    if (argc >= 4) {
        reportPath = argv[3];
    } else {
        const std::filesystem::path stem(annotationPath);
        reportPath = std::filesystem::path("captures") / "validation" /
                     (stem.stem().string() + ".validation.json");
    }

    warzone_audio::AppConfig config;
    if (!warzone_audio::loadConfigFile(configPath, config, &error)) {
        std::cerr << "error=" << error << "\n";
        return 1;
    }

    WavData wav;
    if (!readWav(annotation.clipPath, wav, error)) {
        std::cerr << "error=" << error << "\n";
        return 1;
    }

    const auto scores = processScores(wav, config.engine);
    std::filesystem::create_directories(reportPath.parent_path());
    std::ofstream report(reportPath, std::ios::trunc);
    if (!report) {
        std::cerr << "error=Could not write report " << reportPath.string() << "\n";
        return 1;
    }

    bool allPassed = true;
    report << "{\n";
    report << "  \"schemaVersion\": 1,\n";
    report << "  \"annotation\": \"" << jsonEscape(annotationPath) << "\",\n";
    report << "  \"clip\": \"" << jsonEscape(annotation.clipPath) << "\",\n";
    report << "  \"config\": \"" << jsonEscape(configPath) << "\",\n";
    report << "  \"events\": [\n";

    for (std::size_t i = 0; i < annotation.markers.size(); ++i) {
        const auto& marker = annotation.markers[i];
        const float window = isFootstep(marker.kind) ? 0.350f : 0.500f;
        float maxFootstep = 0.0f;
        float maxProtection = 0.0f;
        float maxAction = 0.0f;
        float maxPeak = 0.0f;
        float minPeak = 1.0f;
        float maxFootstepTime = marker.timeSeconds;
        float maxProtectionTime = marker.timeSeconds;
        float activeMaxPeak = 0.0f;
        float activeMinPeak = 1.0f;
        int activePeakFrames = 0;
        float nearestDelta = 999.0f;
        warzone_audio::DetectorScores nearestScores;
        for (const auto& point : scores) {
            const float delta = std::abs(point.timeSeconds - marker.timeSeconds);
            if (delta < nearestDelta) {
                nearestDelta = delta;
                nearestScores = point.stats.scores;
            }
            if (delta <= window) {
                if (point.stats.scores.footstep > maxFootstep) {
                    maxFootstep = point.stats.scores.footstep;
                    maxFootstepTime = point.timeSeconds;
                }
                if (point.stats.scores.protection > maxProtection) {
                    maxProtection = point.stats.scores.protection;
                    maxProtectionTime = point.timeSeconds;
                }
                maxAction = std::max(maxAction, point.stats.scores.action);
                maxPeak = std::max(maxPeak, point.stats.outputPeak);
                minPeak = std::min(minPeak, point.stats.outputPeak);
            }
        }

        const float activeThreshold = isFootstep(marker.kind) ? std::max(0.35f, maxFootstep * 0.55f) :
                                      isProtectionEvent(marker.kind) ? std::max(0.40f, maxProtection * 0.55f) : 0.0f;
        const float eventCenter = isFootstep(marker.kind) ? maxFootstepTime :
                                  isProtectionEvent(marker.kind) ? maxProtectionTime : marker.timeSeconds;
        for (const auto& point : scores) {
            const float delta = std::abs(point.timeSeconds - eventCenter);
            if (delta > 0.120f) {
                continue;
            }
            const float eventScore = isFootstep(marker.kind) ? point.stats.scores.footstep :
                                     isProtectionEvent(marker.kind) ? point.stats.scores.protection : 1.0f;
            if (eventScore >= activeThreshold) {
                activeMaxPeak = std::max(activeMaxPeak, point.stats.outputPeak);
                activeMinPeak = std::min(activeMinPeak, point.stats.outputPeak);
                activePeakFrames += 1;
            }
        }

        const bool hasThreshold = isFootstep(marker.kind) || isProtectionEvent(marker.kind);
        const bool passed = isFootstep(marker.kind) ? maxFootstep >= 0.60f :
                            isProtectionEvent(marker.kind) ? maxProtection >= 0.70f : true;
        const float peakDropRatio = maxPeak > 0.0001f ? minPeak / maxPeak : 1.0f;
        const float activePeakDropRatio = activeMaxPeak > 0.0001f ? activeMinPeak / activeMaxPeak : 1.0f;
        const bool cutWarning = isFootstep(marker.kind) && activePeakFrames >= 3 &&
                                activeMaxPeak > 0.005f && activePeakDropRatio < 0.12f;
        if (hasThreshold && !passed) {
            allPassed = false;
        }

        report << "    {\n";
        report << "      \"kind\": \"" << jsonEscape(marker.kind) << "\",\n";
        report << "      \"timeSeconds\": " << marker.timeSeconds << ",\n";
        report << "      \"windowSeconds\": " << window << ",\n";
        report << "      \"passed\": " << (passed ? "true" : "false") << ",\n";
        report << "      \"cutWarning\": " << (cutWarning ? "true" : "false") << ",\n";
        report << "      \"maxFootstep\": " << maxFootstep << ",\n";
        report << "      \"maxProtection\": " << maxProtection << ",\n";
        report << "      \"maxAction\": " << maxAction << ",\n";
        report << "      \"maxPeak\": " << maxPeak << ",\n";
        report << "      \"peakDropRatio\": " << peakDropRatio << ",\n";
        report << "      \"activePeakDropRatio\": " << activePeakDropRatio << ",\n";
        report << "      \"activePeakFrames\": " << activePeakFrames << ",\n";
        report << "      \"nearestFootstep\": " << nearestScores.footstep << ",\n";
        report << "      \"nearestProtection\": " << nearestScores.protection << "\n";
        report << "    }" << (i + 1 == annotation.markers.size() ? "\n" : ",\n");
    }

    report << "  ],\n";
    report << "  \"passed\": " << (allPassed ? "true" : "false") << ",\n";
    report << "  \"framesAnalyzed\": " << scores.size() << "\n";
    report << "}\n";
    report.close();

    std::cout << "validation=" << (allPassed ? "PASS" : "FAIL") << "\n";
    std::cout << "events=" << annotation.markers.size() << "\n";
    std::cout << "report=" << reportPath.string() << "\n";
    return allPassed ? 0 : 3;
}
