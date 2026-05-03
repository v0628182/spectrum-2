#include "warzone_audio/Config.h"

#include <algorithm>
#include <cctype>
#include <fstream>
#include <sstream>
#include <stdexcept>
#include <unordered_map>

namespace warzone_audio {

namespace {

std::string trim(std::string value)
{
    const auto notSpace = [](unsigned char ch) { return !std::isspace(ch); };
    value.erase(value.begin(), std::find_if(value.begin(), value.end(), notSpace));
    value.erase(std::find_if(value.rbegin(), value.rend(), notSpace).base(), value.end());
    return value;
}

std::string lower(std::string value)
{
    std::transform(value.begin(), value.end(), value.begin(), [](unsigned char ch) {
        return static_cast<char>(std::tolower(ch));
    });
    return value;
}

bool parseBool(const std::string& value)
{
    const auto v = lower(trim(value));
    return v == "true" || v == "1" || v == "yes" || v == "on";
}

float parseFloat(const std::unordered_map<std::string, std::string>& values,
                 const std::string& key,
                 float fallback)
{
    const auto it = values.find(key);
    if (it == values.end()) {
        return fallback;
    }
    return std::stof(it->second);
}

unsigned parseUnsigned(const std::unordered_map<std::string, std::string>& values,
                       const std::string& key,
                       unsigned fallback)
{
    const auto it = values.find(key);
    if (it == values.end()) {
        return fallback;
    }
    return static_cast<unsigned>(std::stoul(it->second));
}

bool parseBoolValue(const std::unordered_map<std::string, std::string>& values,
                    const std::string& key,
                    bool fallback)
{
    const auto it = values.find(key);
    if (it == values.end()) {
        return fallback;
    }
    return parseBool(it->second);
}

std::string parseString(const std::unordered_map<std::string, std::string>& values,
                        const std::string& key,
                        const std::string& fallback)
{
    const auto it = values.find(key);
    return it == values.end() ? fallback : trim(it->second);
}

} // namespace

bool loadConfigFile(const std::string& path, AppConfig& outConfig, std::string* errorMessage)
{
    try {
        std::ifstream input(path);
        if (!input) {
            if (errorMessage) {
                *errorMessage = "Could not open config file: " + path;
            }
            return false;
        }

        std::unordered_map<std::string, std::string> values;
        std::string section;
        std::string line;
        unsigned lineNumber = 0;

        while (std::getline(input, line)) {
            ++lineNumber;
            const auto comment = line.find_first_of("#;");
            if (comment != std::string::npos) {
                line = line.substr(0, comment);
            }
            line = trim(line);
            if (line.empty()) {
                continue;
            }
            if (line.front() == '[' && line.back() == ']') {
                section = lower(trim(line.substr(1, line.size() - 2)));
                continue;
            }
            const auto eq = line.find('=');
            if (eq == std::string::npos) {
                if (errorMessage) {
                    *errorMessage = "Invalid config line " + std::to_string(lineNumber);
                }
                return false;
            }
            const auto key = lower(section + "." + trim(line.substr(0, eq)));
            const auto value = trim(line.substr(eq + 1));
            values[key] = value;
        }

        AppConfig config = outConfig;
        config.engine.footstepEnhance = parseFloat(values, "audio.footstepenhance", config.engine.footstepEnhance);
        config.engine.actionDetail = parseFloat(values, "audio.actiondetail", config.engine.actionDetail);
        config.engine.gunshotReduction = parseFloat(values, "audio.gunshotreduction", config.engine.gunshotReduction);
        config.engine.explosionReduction = parseFloat(values, "audio.explosionreduction", config.engine.explosionReduction);
        config.engine.detectionSensitivity =
            parseFloat(values, "audio.detectionsensitivity", config.engine.detectionSensitivity);
        config.engine.outputCeilingDb = parseFloat(values, "audio.outputceilingdb", config.engine.outputCeilingDb);
        config.engine.stepBodyBoostDb = parseFloat(values, "audio.stepbodyboostdb", config.engine.stepBodyBoostDb);
        config.engine.stepClarityBoostDb =
            parseFloat(values, "audio.stepclarityboostdb", config.engine.stepClarityBoostDb);
        config.engine.stepLowBodyBoostDb =
            parseFloat(values, "audio.steplowbodyboostdb", config.engine.stepLowBodyBoostDb);
        config.engine.stepLowMidBoostDb =
            parseFloat(values, "audio.steplowmidboostdb", config.engine.stepLowMidBoostDb);
        config.engine.weaponMidCutDb = parseFloat(values, "audio.weaponmidcutdb", config.engine.weaponMidCutDb);
        config.engine.weaponAirCutDb = parseFloat(values, "audio.weaponaircutdb", config.engine.weaponAirCutDb);
        config.engine.sustainedHoldMs = parseFloat(values, "audio.sustainedholdms", config.engine.sustainedHoldMs);
        config.engine.masterDuckDb = parseFloat(values, "audio.masterduckdb", config.engine.masterDuckDb);
        config.engine.impactDuckDb = parseFloat(values, "audio.impactduckdb", config.engine.impactDuckDb);
        config.engine.footstepLevelerAmount =
            parseFloat(values, "audio.footstepleveleramount", config.engine.footstepLevelerAmount);
        config.engine.footstepTargetRmsDb =
            parseFloat(values, "audio.footsteptargetrmsdb", config.engine.footstepTargetRmsDb);
        config.engine.footstepMaxLiftDb =
            parseFloat(values, "audio.footstepmaxliftdb", config.engine.footstepMaxLiftDb);
        config.engine.footstepLevelerSpeedMs =
            parseFloat(values, "audio.footsteplevelerspeedms", config.engine.footstepLevelerSpeedMs);
        config.engine.stabilityAmount = parseFloat(values, "audio.stabilityamount", config.engine.stabilityAmount);
        config.engine.spectralFloorDb = parseFloat(values, "audio.spectralfloordb", config.engine.spectralFloorDb);
        config.engine.stableReleaseMs = parseFloat(values, "audio.stablereleasems", config.engine.stableReleaseMs);
        config.engine.footstepGuardAmount =
            parseFloat(values, "audio.footstepguardamount", config.engine.footstepGuardAmount);
        config.engine.maxCutStepDb = parseFloat(values, "audio.maxcutstepdb", config.engine.maxCutStepDb);
        config.engine.protectionExtreme =
            parseBoolValue(values, "audio.protectionextreme", config.engine.protectionExtreme);
        config.engine.debugLogging = parseBoolValue(values, "audio.debuglogging", config.engine.debugLogging);
        config.logging.logPath = parseString(values, "logging.logpath", config.logging.logPath);
        config.logging.logEveryFrames = parseUnsigned(values, "logging.logeveryframes", config.logging.logEveryFrames);

        outConfig = config;
        return true;
    } catch (const std::exception& ex) {
        if (errorMessage) {
            *errorMessage = ex.what();
        }
        return false;
    }
}

bool saveConfigFile(const std::string& path, const AppConfig& config, std::string* errorMessage)
{
    std::ofstream output(path, std::ios::trunc);
    if (!output) {
        if (errorMessage) {
            *errorMessage = "Could not write config file: " + path;
        }
        return false;
    }

    output << "[audio]\n";
    output << "footstepEnhance=" << config.engine.footstepEnhance << "\n";
    output << "actionDetail=" << config.engine.actionDetail << "\n";
    output << "gunshotReduction=" << config.engine.gunshotReduction << "\n";
    output << "explosionReduction=" << config.engine.explosionReduction << "\n";
    output << "detectionSensitivity=" << config.engine.detectionSensitivity << "\n";
    output << "outputCeilingDb=" << config.engine.outputCeilingDb << "\n";
    output << "stepBodyBoostDb=" << config.engine.stepBodyBoostDb << "\n";
    output << "stepClarityBoostDb=" << config.engine.stepClarityBoostDb << "\n";
    output << "stepLowBodyBoostDb=" << config.engine.stepLowBodyBoostDb << "\n";
    output << "stepLowMidBoostDb=" << config.engine.stepLowMidBoostDb << "\n";
    output << "weaponMidCutDb=" << config.engine.weaponMidCutDb << "\n";
    output << "weaponAirCutDb=" << config.engine.weaponAirCutDb << "\n";
    output << "sustainedHoldMs=" << config.engine.sustainedHoldMs << "\n";
    output << "masterDuckDb=" << config.engine.masterDuckDb << "\n";
    output << "impactDuckDb=" << config.engine.impactDuckDb << "\n";
    output << "footstepLevelerAmount=" << config.engine.footstepLevelerAmount << "\n";
    output << "footstepTargetRmsDb=" << config.engine.footstepTargetRmsDb << "\n";
    output << "footstepMaxLiftDb=" << config.engine.footstepMaxLiftDb << "\n";
    output << "footstepLevelerSpeedMs=" << config.engine.footstepLevelerSpeedMs << "\n";
    output << "stabilityAmount=" << config.engine.stabilityAmount << "\n";
    output << "spectralFloorDb=" << config.engine.spectralFloorDb << "\n";
    output << "stableReleaseMs=" << config.engine.stableReleaseMs << "\n";
    output << "footstepGuardAmount=" << config.engine.footstepGuardAmount << "\n";
    output << "maxCutStepDb=" << config.engine.maxCutStepDb << "\n";
    output << "protectionExtreme=" << (config.engine.protectionExtreme ? "true" : "false") << "\n";
    output << "debugLogging=" << (config.engine.debugLogging ? "true" : "false") << "\n\n";
    output << "[logging]\n";
    output << "logPath=" << config.logging.logPath << "\n";
    output << "logEveryFrames=" << config.logging.logEveryFrames << "\n";
    return true;
}

} // namespace warzone_audio
