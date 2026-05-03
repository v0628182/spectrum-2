#pragma once

#include <fstream>
#include <string>

#include "warzone_audio/DspTypes.h"

namespace warzone_audio {

class ScoreLogger {
public:
    ScoreLogger() = default;

    bool open(const std::string& path, unsigned logEveryFrames);
    void close();
    bool isOpen() const;
    void write(const ProcessStats& stats);

private:
    std::ofstream output_;
    unsigned logEveryFrames_ = 8;
    std::size_t lastLoggedFrame_ = 0;
};

} // namespace warzone_audio
