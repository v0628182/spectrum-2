#include "warzone_audio/DspEngine.h"

#include <algorithm>
#include <array>
#include <memory>

#include "FeatureExtractor.h"
#include "Processor.h"
#include "TransientDetector.h"

namespace warzone_audio {

struct DspEngine::Impl {
    EngineParams params;
    FeatureExtractor extractor;
    TransientDetector detector;
    Processor processor;
    ProcessStats stats;
    std::array<float, constants::kFftSize> windowL{};
    std::array<float, constants::kFftSize> windowR{};
    std::size_t writeIndex = 0;
    std::size_t samplesSinceHop = 0;
    std::size_t framesAnalyzed = 0;
};

DspEngine::DspEngine()
    : impl_(std::make_unique<Impl>())
{
    reset();
}

DspEngine::~DspEngine() = default;
DspEngine::DspEngine(DspEngine&&) noexcept = default;
DspEngine& DspEngine::operator=(DspEngine&&) noexcept = default;

void DspEngine::reset()
{
    impl_->extractor.reset();
    impl_->detector.reset();
    impl_->processor.reset();
    impl_->stats = {};
    impl_->windowL.fill(0.0f);
    impl_->windowR.fill(0.0f);
    impl_->writeIndex = 0;
    impl_->samplesSinceHop = 0;
    impl_->framesAnalyzed = 0;
}

void DspEngine::setParams(const EngineParams& params)
{
    impl_->params = params;
}

const EngineParams& DspEngine::params() const
{
    return impl_->params;
}

void DspEngine::processBlock(const float* inL, const float* inR, float* outL, float* outR, std::size_t numSamples)
{
    float peak = 0.0f;
    for (std::size_t i = 0; i < numSamples; ++i) {
        impl_->windowL[impl_->writeIndex] = inL[i];
        impl_->windowR[impl_->writeIndex] = inR[i];
        impl_->writeIndex = (impl_->writeIndex + 1) % constants::kFftSize;
        impl_->samplesSinceHop += 1;

        if (impl_->samplesSinceHop >= constants::kHopSize) {
            std::array<float, constants::kFftSize> orderedL{};
            std::array<float, constants::kFftSize> orderedR{};
            for (std::size_t n = 0; n < constants::kFftSize; ++n) {
                const std::size_t idx = (impl_->writeIndex + n) % constants::kFftSize;
                orderedL[n] = impl_->windowL[idx];
                orderedR[n] = impl_->windowR[idx];
            }

            impl_->stats.features = impl_->extractor.analyze(orderedL, orderedR);
            impl_->stats.scores = impl_->detector.update(impl_->stats.features, impl_->params);
            impl_->processor.updateTargets(impl_->stats.scores, impl_->params);
            impl_->samplesSinceHop = 0;
            impl_->framesAnalyzed += 1;
        }

        impl_->processor.processSample(inL[i], inR[i], outL[i], outR[i], peak);
    }

    impl_->stats.outputPeak = peak;
    impl_->stats.framesAnalyzed = impl_->framesAnalyzed;
}

const ProcessStats& DspEngine::stats() const
{
    return impl_->stats;
}

} // namespace warzone_audio
