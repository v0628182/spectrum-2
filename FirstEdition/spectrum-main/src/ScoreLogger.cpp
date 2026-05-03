#include "warzone_audio/ScoreLogger.h"

#include <filesystem>

namespace warzone_audio {

bool ScoreLogger::open(const std::string& path, unsigned logEveryFrames)
{
    close();
    logEveryFrames_ = logEveryFrames == 0 ? 1 : logEveryFrames;
    lastLoggedFrame_ = 0;

    const std::filesystem::path logPath(path);
    if (logPath.has_parent_path()) {
        std::filesystem::create_directories(logPath.parent_path());
    }

    output_.open(path, std::ios::trunc);
    if (!output_) {
        return false;
    }

    output_ << "frame,footstep,action,protection,lateral,confidence,outputPeak,"
            << "energyStepDb,energyLowMidDb,energyBassDb,snrStepDb,superFluxStep,superFluxStepExcess,centroidHz,flatnessStep,crestDb,inputPeak\n";
    return true;
}

void ScoreLogger::close()
{
    if (output_.is_open()) {
        output_.close();
    }
}

bool ScoreLogger::isOpen() const
{
    return output_.is_open();
}

void ScoreLogger::write(const ProcessStats& stats)
{
    if (!output_.is_open() || stats.framesAnalyzed == lastLoggedFrame_) {
        return;
    }
    if ((stats.framesAnalyzed % logEveryFrames_) != 0) {
        return;
    }

    lastLoggedFrame_ = stats.framesAnalyzed;
    output_ << stats.framesAnalyzed << ','
            << stats.scores.footstep << ','
            << stats.scores.action << ','
            << stats.scores.protection << ','
            << stats.scores.lateral << ','
            << stats.scores.confidence << ','
            << stats.outputPeak << ','
            << stats.features.energyDb.step << ','
            << stats.features.energyDb.lowMid << ','
            << stats.features.energyDb.bass << ','
            << stats.features.snrDb.step << ','
            << stats.features.superFluxStep << ','
            << stats.features.superFluxStepExcess << ','
            << stats.features.centroidHz << ','
            << stats.features.flatnessStep << ','
            << stats.features.crestDb << ','
            << stats.features.inputPeak << '\n';
}

} // namespace warzone_audio
