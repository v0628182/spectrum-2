#include "WarzoneApoSkeleton.h"

#if defined(WARZONE_APO_WDK_BUILD)

#include <algorithm>

namespace {

void copyFramesIfNeeded(const float* input, float* output, UINT32 frames, UINT32 channels) noexcept
{
    if (input != output) {
        std::copy(input, input + static_cast<std::size_t>(frames) * channels, output);
    }
}

void writeSilence(float* output, UINT32 frames, UINT32 channels) noexcept
{
    std::fill(output, output + static_cast<std::size_t>(frames) * channels, 0.0f);
}

} // namespace

CWarzoneApoMfx::CWarzoneApoMfx()
{
    realtime_.prepare(maxFrames_, 8);
}

bool CWarzoneApoMfx::supportsFormat(const WAVEFORMATEX* format) const noexcept
{
    if (!format) {
        return false;
    }

    const bool isFloat = (format->wFormatTag == WAVE_FORMAT_IEEE_FLOAT) ||
                         (format->wFormatTag == WAVE_FORMAT_EXTENSIBLE &&
                          reinterpret_cast<const WAVEFORMATEXTENSIBLE*>(format)->SubFormat == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT);
    return isFloat &&
           format->nSamplesPerSec == 48000 &&
           format->wBitsPerSample == 32 &&
           format->nChannels >= 1 &&
           format->nChannels <= 8;
}

STDMETHODIMP_(void) CWarzoneApoMfx::APOProcess(UINT32 numInputConnections,
                                               APO_CONNECTION_PROPERTY** inputConnections,
                                               UINT32 numOutputConnections,
                                               APO_CONNECTION_PROPERTY** outputConnections)
{
    UNREFERENCED_PARAMETER(numInputConnections);
    UNREFERENCED_PARAMETER(numOutputConnections);

    if (!locked_ || !inputConnections || !outputConnections || !inputConnections[0] || !outputConnections[0]) {
        return;
    }

    auto* input = reinterpret_cast<float*>(inputConnections[0]->pBuffer);
    auto* output = reinterpret_cast<float*>(outputConnections[0]->pBuffer);
    const UINT32 frames = inputConnections[0]->u32ValidFrameCount;

    switch (inputConnections[0]->u32BufferFlags) {
    case BUFFER_VALID:
        realtime_.processInterleaved(input, output, frames, samplesPerFrame());
        outputConnections[0]->u32BufferFlags = BUFFER_VALID;
        outputConnections[0]->u32ValidFrameCount = frames;
        break;
    case BUFFER_SILENT:
        writeSilence(output, frames, samplesPerFrame());
        outputConnections[0]->u32BufferFlags = BUFFER_SILENT;
        outputConnections[0]->u32ValidFrameCount = frames;
        break;
    case BUFFER_INVALID:
    default:
        copyFramesIfNeeded(input, output, frames, samplesPerFrame());
        outputConnections[0]->u32BufferFlags = inputConnections[0]->u32BufferFlags;
        outputConnections[0]->u32ValidFrameCount = frames;
        break;
    }
}

STDMETHODIMP CWarzoneApoMfx::GetLatency(HNSTIME* latency)
{
    if (!latency) {
        return E_POINTER;
    }
    *latency = 0;
    return S_OK;
}

STDMETHODIMP CWarzoneApoMfx::LockForProcess(UINT32 numInputConnections,
                                            APO_CONNECTION_DESCRIPTOR** inputConnections,
                                            UINT32 numOutputConnections,
                                            APO_CONNECTION_DESCRIPTOR** outputConnections)
{
    HRESULT hr = CBaseAudioProcessingObject::LockForProcess(numInputConnections,
                                                            inputConnections,
                                                            numOutputConnections,
                                                            outputConnections);
    if (FAILED(hr)) {
        return hr;
    }

    if (!inputConnections || !inputConnections[0] || !supportsFormat(inputConnections[0]->pFormat)) {
        CBaseAudioProcessingObject::UnlockForProcess();
        return APOERR_INVALID_CONNECTION_FORMAT;
    }

    channels_ = inputConnections[0]->pFormat->nChannels;
    maxFrames_ = inputConnections[0]->u32MaxFrameCount;
    if (!realtime_.prepare(maxFrames_, 8)) {
        CBaseAudioProcessingObject::UnlockForProcess();
        return E_OUTOFMEMORY;
    }

    locked_ = true;
    return S_OK;
}

STDMETHODIMP CWarzoneApoMfx::UnlockForProcess()
{
    locked_ = false;
    return CBaseAudioProcessingObject::UnlockForProcess();
}

STDMETHODIMP CWarzoneApoMfx::IsInputFormatSupported(IAudioMediaType* outputFormat,
                                                    IAudioMediaType* requestedInputFormat,
                                                    IAudioMediaType** supportedInputFormat)
{
    if (supportedInputFormat) {
        *supportedInputFormat = nullptr;
    }
    if (!outputFormat || !requestedInputFormat) {
        return E_POINTER;
    }

    UNCOMPRESSEDAUDIOFORMAT requested = {};
    HRESULT hr = requestedInputFormat->GetUncompressedAudioFormat(&requested);
    if (FAILED(hr)) {
        return hr;
    }

    if (requested.fFramesPerSecond != 48000 ||
        requested.dwSamplesPerFrame < 1 ||
        requested.dwSamplesPerFrame > 8 ||
        requested.dwBytesPerSampleContainer != sizeof(float) ||
        requested.dwValidBitsPerSample != 32) {
        return APOERR_FORMAT_NOT_SUPPORTED;
    }

    return S_OK;
}

STDMETHODIMP CWarzoneApoMfx::GetEffectsList(GUID** effectsIds, UINT* effectCount, HANDLE event)
{
    UNREFERENCED_PARAMETER(event);
    if (!effectsIds || !effectCount) {
        return E_POINTER;
    }
    *effectsIds = nullptr;
    *effectCount = 0;
    return S_OK;
}

void CWarzoneApoMfx::SetParamsFromControlBridge(const warzone_audio::EngineParams& params) noexcept
{
    realtime_.setParams(params);
}

warzone_audio::RealtimeSnapshot CWarzoneApoMfx::GetSnapshotForControlBridge() const noexcept
{
    return realtime_.snapshot();
}

#endif // WARZONE_APO_WDK_BUILD
