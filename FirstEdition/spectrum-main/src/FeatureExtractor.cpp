#include "FeatureExtractor.h"

#include <algorithm>
#include <cmath>

#include "MathUtils.h"

namespace warzone_audio {

namespace {

float rangeAverage(const std::array<float, constants::kPositiveBins>& values, BinRange range)
{
    float sum = 0.0f;
    for (std::size_t k = range.first; k <= range.last; ++k) {
        sum += values[k];
    }
    return sum / static_cast<float>(range.last - range.first + 1);
}

int countActiveBands(const BandEnergiesDb& snr)
{
    int active = 0;
    active += snr.bass > 10.0f ? 1 : 0;
    active += snr.lowMid > 10.0f ? 1 : 0;
    active += snr.mid > 10.0f ? 1 : 0;
    active += snr.step > 10.0f ? 1 : 0;
    active += snr.air > 10.0f ? 1 : 0;
    return active;
}

} // namespace

FeatureExtractor::FeatureExtractor()
    : fft_(constants::kFftSize),
      fftBuffer_(constants::kFftSize)
{
    for (std::size_t n = 0; n < constants::kFftSize; ++n) {
        window_[n] = 0.5f - 0.5f * std::cos(2.0f * constants::kPi * static_cast<float>(n) /
                                           static_cast<float>(constants::kFftSize));
    }
    reset();
}

void FeatureExtractor::reset()
{
    prevLogMag_.fill(std::log(constants::kEpsAmp));
    noiseDb_.bass = -90.0f;
    noiseDb_.lowMid = -90.0f;
    noiseDb_.mid = -90.0f;
    noiseDb_.step = -90.0f;
    noiseDb_.air = -90.0f;
    noiseDb_.noise = -90.0f;
    noiseDb_.total = -80.0f;
    slowEnergyDb_ = noiseDb_;
    fluxNoiseStep_ = 0.0f;
    fluxNoisePresence_ = 0.0f;
    fluxNoiseBroadband_ = 0.0f;
    activeStepFrames_ = 0.0f;
    warmupFrames_ = 0;
    hasPrevious_ = false;
}

float FeatureExtractor::bandEnergyDb(const std::array<float, constants::kPositiveBins>& power, BinRange range) const
{
    return powerToDb(rangeAverage(power, range));
}

float FeatureExtractor::bandSuperFlux(const std::array<float, constants::kPositiveBins>& logMag, BinRange range) const
{
    float sum = 0.0f;
    for (std::size_t k = range.first; k <= range.last; ++k) {
        float maxPrev = prevLogMag_[k];
        const int first = std::max<int>(1, static_cast<int>(k) - constants::kSuperFluxRadiusBins);
        const int last = std::min<int>(static_cast<int>(constants::kPositiveBins) - 1,
                                       static_cast<int>(k) + constants::kSuperFluxRadiusBins);
        for (int p = first; p <= last; ++p) {
            maxPrev = std::max(maxPrev, prevLogMag_[static_cast<std::size_t>(p)]);
        }
        sum += std::max(0.0f, logMag[k] - maxPrev);
    }
    return hasPrevious_ ? sum / static_cast<float>(range.last - range.first + 1) : 0.0f;
}

float FeatureExtractor::updateNoise(float currentNoiseDb, float energyDb) const
{
    if (energyDb < currentNoiseDb) {
        return 0.90f * currentNoiseDb + 0.10f * energyDb;
    }
    return smooth(currentNoiseDb, energyDb, constants::kTauNoiseMs);
}

FeatureFrame FeatureExtractor::analyze(const std::array<float, constants::kFftSize>& left,
                                       const std::array<float, constants::kFftSize>& right)
{
    float peak = 0.0f;
    float sumSq = 0.0f;
    for (std::size_t n = 0; n < constants::kFftSize; ++n) {
        const float mid = 0.5f * (left[n] + right[n]);
        peak = std::max(peak, std::abs(mid));
        sumSq += mid * mid;
        fftBuffer_[n] = std::complex<float>(mid * window_[n], 0.0f);
    }

    fft_.forward(fftBuffer_);

    for (std::size_t k = 0; k < constants::kPositiveBins; ++k) {
        magMid_[k] = std::abs(fftBuffer_[k]);
        powMid_[k] = magMid_[k] * magMid_[k];
        logMagMid_[k] = std::log(std::max(magMid_[k], constants::kEpsAmp));
    }

    FeatureFrame frame;
    frame.energyDb.bass = bandEnergyDb(powMid_, constants::kBassBins);
    frame.energyDb.lowMid = bandEnergyDb(powMid_, constants::kLowMidBins);
    frame.energyDb.mid = bandEnergyDb(powMid_, constants::kMidBins);
    frame.energyDb.step = bandEnergyDb(powMid_, constants::kStepBins);
    frame.energyDb.air = bandEnergyDb(powMid_, constants::kAirBins);
    frame.energyDb.noise = bandEnergyDb(powMid_, constants::kNoiseBins);

    float totalPower = 0.0f;
    float weightedHz = 0.0f;
    for (std::size_t k = 1; k <= constants::kNoiseBins.last; ++k) {
        totalPower += powMid_[k];
        weightedHz += powMid_[k] * constants::kBinHz * static_cast<float>(k);
    }
    frame.energyDb.total = powerToDb(totalPower);
    frame.centroidHz = weightedHz / (totalPower + constants::kEpsEnergy);

    if (warmupFrames_ < 16) {
        const float a = warmupFrames_ == 0 ? 0.0f : 0.75f;
        noiseDb_.bass = a * noiseDb_.bass + (1.0f - a) * frame.energyDb.bass;
        noiseDb_.lowMid = a * noiseDb_.lowMid + (1.0f - a) * frame.energyDb.lowMid;
        noiseDb_.mid = a * noiseDb_.mid + (1.0f - a) * frame.energyDb.mid;
        noiseDb_.step = a * noiseDb_.step + (1.0f - a) * frame.energyDb.step;
        noiseDb_.air = a * noiseDb_.air + (1.0f - a) * frame.energyDb.air;
        noiseDb_.noise = a * noiseDb_.noise + (1.0f - a) * frame.energyDb.noise;
        noiseDb_.total = a * noiseDb_.total + (1.0f - a) * frame.energyDb.total;
        warmupFrames_ += 1;
    } else {
        noiseDb_.bass = updateNoise(noiseDb_.bass, frame.energyDb.bass);
        noiseDb_.lowMid = updateNoise(noiseDb_.lowMid, frame.energyDb.lowMid);
        noiseDb_.mid = updateNoise(noiseDb_.mid, frame.energyDb.mid);
        noiseDb_.step = updateNoise(noiseDb_.step, frame.energyDb.step);
        noiseDb_.air = updateNoise(noiseDb_.air, frame.energyDb.air);
        noiseDb_.noise = updateNoise(noiseDb_.noise, frame.energyDb.noise);
        noiseDb_.total = updateNoise(noiseDb_.total, frame.energyDb.total);
    }
    frame.noiseDb = noiseDb_;

    frame.snrDb.bass = frame.energyDb.bass - noiseDb_.bass;
    frame.snrDb.lowMid = frame.energyDb.lowMid - noiseDb_.lowMid;
    frame.snrDb.mid = frame.energyDb.mid - noiseDb_.mid;
    frame.snrDb.step = frame.energyDb.step - noiseDb_.step;
    frame.snrDb.air = frame.energyDb.air - noiseDb_.air;
    frame.snrDb.noise = frame.energyDb.noise - noiseDb_.noise;
    frame.snrDb.total = frame.energyDb.total - noiseDb_.total;

    frame.superFluxStep = bandSuperFlux(logMagMid_, constants::kStepBins);
    frame.superFluxPresence = bandSuperFlux(logMagMid_, constants::kMidBins);
    frame.superFluxBroadband = (bandSuperFlux(logMagMid_, constants::kBassBins) +
                                bandSuperFlux(logMagMid_, constants::kLowMidBins) +
                                bandSuperFlux(logMagMid_, constants::kMidBins) +
                                frame.superFluxStep +
                                bandSuperFlux(logMagMid_, constants::kAirBins)) /
                               5.0f;

    if (warmupFrames_ < 16) {
        fluxNoiseStep_ = 0.8f * fluxNoiseStep_ + 0.2f * frame.superFluxStep;
        fluxNoisePresence_ = 0.8f * fluxNoisePresence_ + 0.2f * frame.superFluxPresence;
        fluxNoiseBroadband_ = 0.8f * fluxNoiseBroadband_ + 0.2f * frame.superFluxBroadband;
    } else {
        fluxNoiseStep_ = smooth(fluxNoiseStep_, frame.superFluxStep, constants::kTauSlowMs);
        fluxNoisePresence_ = smooth(fluxNoisePresence_, frame.superFluxPresence, constants::kTauSlowMs);
        fluxNoiseBroadband_ = smooth(fluxNoiseBroadband_, frame.superFluxBroadband, constants::kTauSlowMs);
    }
    frame.superFluxStepExcess = std::max(0.0f, frame.superFluxStep - fluxNoiseStep_);
    frame.superFluxPresenceExcess = std::max(0.0f, frame.superFluxPresence - fluxNoisePresence_);
    frame.superFluxBroadbandExcess = std::max(0.0f, frame.superFluxBroadband - fluxNoiseBroadband_);

    float geoLog = 0.0f;
    for (std::size_t k = constants::kStepBins.first; k <= constants::kStepBins.last; ++k) {
        geoLog += std::log(powMid_[k] + constants::kEpsEnergy);
    }
    geoLog /= static_cast<float>(constants::kStepBins.last - constants::kStepBins.first + 1);
    const float arith = rangeAverage(powMid_, constants::kStepBins);
    frame.flatnessStep = std::exp(geoLog) / (arith + constants::kEpsEnergy);

    const float rms = std::sqrt(sumSq / static_cast<float>(constants::kFftSize) + constants::kEpsEnergy);
    frame.crestDb = ampToDb(peak / rms);
    frame.inputPeak = peak;

    frame.attackStepDb = frame.energyDb.step - slowEnergyDb_.step;
    frame.attackLowMidDb = frame.energyDb.lowMid - slowEnergyDb_.lowMid;
    slowEnergyDb_.step = smooth(slowEnergyDb_.step, frame.energyDb.step, constants::kTauSlowMs);
    slowEnergyDb_.lowMid = smooth(slowEnergyDb_.lowMid, frame.energyDb.lowMid, constants::kTauSlowMs);

    if (frame.snrDb.step > 6.0f) {
        activeStepFrames_ += 1.0f;
    } else {
        activeStepFrames_ = 0.0f;
    }
    frame.durationMs = activeStepFrames_ * constants::kHopMs;
    frame.activeBands = countActiveBands(frame.snrDb);

    float eL = 0.0f;
    float eR = 0.0f;
    for (std::size_t n = 0; n < constants::kFftSize; ++n) {
        fftBuffer_[n] = std::complex<float>(left[n] * window_[n], 0.0f);
    }
    fft_.forward(fftBuffer_);
    for (std::size_t k = constants::kStepBins.first; k <= constants::kStepBins.last; ++k) {
        eL += std::norm(fftBuffer_[k]);
    }
    for (std::size_t n = 0; n < constants::kFftSize; ++n) {
        fftBuffer_[n] = std::complex<float>(right[n] * window_[n], 0.0f);
    }
    fft_.forward(fftBuffer_);
    for (std::size_t k = constants::kStepBins.first; k <= constants::kStepBins.last; ++k) {
        eR += std::norm(fftBuffer_[k]);
    }
    frame.lateral = (eR - eL) / (eR + eL + constants::kEpsEnergy);

    prevLogMag_ = logMagMid_;
    hasPrevious_ = true;
    return frame;
}

} // namespace warzone_audio
