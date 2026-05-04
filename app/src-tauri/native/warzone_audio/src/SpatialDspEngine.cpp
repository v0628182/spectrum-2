#include "warzone_audio/SpatialDspEngine.h"

#include <algorithm>
#include <array>
#include <memory>

#include "FeatureExtractor.h"
#include "SelfWeaponSuppressor.h"
#include "SpatialTypes.h"
#include "TransientDetector.h"
#include "MathUtils.h"

namespace warzone_audio {

namespace {

float mixSafe(float value)
{
    return clamp(value, -1.0f, 1.0f);
}

struct AnalysisFrame {
    float left = 0.0f;
    float right = 0.0f;
};

AnalysisFrame analysisFrameFromSpatial(const float* frame, std::size_t channels, const SpatialLayout& layout)
{
    AnalysisFrame out;
    if (channels == 0) {
        return out;
    }

    const int fl = layout.frontLeft();
    const int fr = layout.frontRight();
    const int fc = layout.frontCenter();

    if (channels == 1) {
        out.left = frame[0];
        out.right = frame[0];
        return out;
    }

    const float left = fl >= 0 ? frame[fl] : frame[0];
    const float right = fr >= 0 ? frame[fr] : frame[std::min<std::size_t>(1, channels - 1)];
    const float center = fc >= 0 ? frame[fc] : 0.5f * (left + right);

    out.left = mixSafe(left + 0.55f * center);
    out.right = mixSafe(right + 0.55f * center);
    return out;
}

} // namespace

struct SpatialDspEngine::Impl {
    EngineParams params;
    FeatureExtractor extractor;
    TransientDetector detector;
    SelfWeaponSuppressor suppressor;
    ProcessStats stats;
    std::array<float, constants::kFftSize> windowL{};
    std::array<float, constants::kFftSize> windowR{};
    std::array<float, 8> frameIn{};
    std::array<float, 8> frameOut{};
    SpatialLayout layout{};
    std::uint32_t lastMask = 0;
    std::size_t lastChannels = 0;
    std::size_t writeIndex = 0;
    std::size_t samplesSinceHop = 0;
    std::size_t framesAnalyzed = 0;
};

SpatialDspEngine::SpatialDspEngine()
    : impl_(std::make_unique<Impl>())
{
    reset();
}

SpatialDspEngine::~SpatialDspEngine() = default;
SpatialDspEngine::SpatialDspEngine(SpatialDspEngine&&) noexcept = default;
SpatialDspEngine& SpatialDspEngine::operator=(SpatialDspEngine&&) noexcept = default;

void SpatialDspEngine::reset()
{
    impl_->extractor.reset();
    impl_->detector.reset();
    impl_->suppressor.reset();
    impl_->stats = {};
    impl_->windowL.fill(0.0f);
    impl_->windowR.fill(0.0f);
    impl_->frameIn.fill(0.0f);
    impl_->frameOut.fill(0.0f);
    impl_->layout = {};
    impl_->lastMask = 0;
    impl_->lastChannels = 0;
    impl_->writeIndex = 0;
    impl_->samplesSinceHop = 0;
    impl_->framesAnalyzed = 0;
}

void SpatialDspEngine::setParams(const EngineParams& params)
{
    impl_->params = params;
    impl_->suppressor.updateTargets(impl_->stats.scores, impl_->params);
}

const EngineParams& SpatialDspEngine::params() const
{
    return impl_->params;
}

void SpatialDspEngine::processInterleaved(const float* input,
                                          float* output,
                                          std::size_t frames,
                                          std::size_t channels,
                                          std::uint32_t channelMask)
{
    if (!input || !output || frames == 0 || channels == 0) {
        return;
    }

    if (channels > impl_->frameIn.size()) {
        if (input != output) {
            std::copy(input, input + frames * channels, output);
        }
        return;
    }

    if (channels != impl_->lastChannels || channelMask != impl_->lastMask) {
        impl_->layout = resolveLayout(channels, channelMask);
        impl_->lastChannels = channels;
        impl_->lastMask = channelMask;
    }

    float peak = 0.0f;
    const float wetMix = clamp(impl_->params.wetMix / 100.0f, 0.0f, 1.0f);

    for (std::size_t i = 0; i < frames; ++i) {
        const float* frame = input + i * channels;
        float* outFrame = output + i * channels;

        for (std::size_t ch = 0; ch < channels; ++ch) {
            impl_->frameIn[ch] = frame[ch];
        }

        const auto analysis = analysisFrameFromSpatial(impl_->frameIn.data(), channels, impl_->layout);
        impl_->windowL[impl_->writeIndex] = analysis.left;
        impl_->windowR[impl_->writeIndex] = analysis.right;
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
            impl_->suppressor.updateTargets(impl_->stats.scores, impl_->params);
            impl_->samplesSinceHop = 0;
            impl_->framesAnalyzed += 1;
        }

        impl_->suppressor.processFrame(impl_->frameIn.data(),
                                       impl_->frameOut.data(),
                                       channels,
                                       impl_->layout,
                                       wetMix,
                                       peak);

        for (std::size_t ch = 0; ch < channels; ++ch) {
            outFrame[ch] = impl_->frameOut[ch];
        }
    }

    const auto& suppressorStats = impl_->suppressor.snapshot();
    impl_->stats.scores.protection = std::max(impl_->stats.scores.protection, suppressorStats.weaponMask);
    impl_->stats.scores.footstep = std::max(impl_->stats.scores.footstep, suppressorStats.protectMask);
    impl_->stats.scores.confidence =
        std::max({impl_->stats.scores.confidence, suppressorStats.weaponMask, suppressorStats.protectMask});
    impl_->stats.outputPeak = peak;
    impl_->stats.framesAnalyzed = impl_->framesAnalyzed;
}

const ProcessStats& SpatialDspEngine::stats() const
{
    return impl_->stats;
}

} // namespace warzone_audio
