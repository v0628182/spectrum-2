#pragma once

namespace warzone_audio::control {

constexpr const char* kSetParams = "setParams";
constexpr const char* kLoadPreset = "loadPreset";
constexpr const char* kRequestStats = "requestStats";
constexpr const char* kEnableDebug = "enableDebug";

constexpr const char* kPipeName = R"(\\.\pipe\warzone_audio_control)";
constexpr const char* kSharedMemoryName = "Local\\WarzoneAudioParams";

} // namespace warzone_audio::control
