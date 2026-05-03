#pragma once

#include <string>

#include "warzone_audio/DspTypes.h"

namespace warzone_audio {

struct LogConfig {
    std::string logPath = "logs/warzone_audio_dsp.csv";
    unsigned logEveryFrames = 8;
};

struct AppConfig {
    EngineParams engine;
    LogConfig logging;
};

bool loadConfigFile(const std::string& path, AppConfig& outConfig, std::string* errorMessage = nullptr);
bool saveConfigFile(const std::string& path, const AppConfig& config, std::string* errorMessage = nullptr);

} // namespace warzone_audio
