#pragma once

// This file is a WDK scaffold. It is excluded from the portable g++ build and
// should be compiled inside a Visual Studio/WDK APO project based on SYSVAD
// SwapAPO.

#if defined(WARZONE_APO_WDK_BUILD)

#include <Windows.h>
#include <audioenginebaseapo.h>
#include <baseaudioprocessingobject.h>

#include "warzone_audio/RealtimeEngine.h"

// Replace these GUIDs before production packaging.
// {8F5CC6E6-9B13-43C5-AF42-4C0C7A0B8AF1}
DEFINE_GUID(CLSID_WarzoneApoMfx,
            0x8f5cc6e6, 0x9b13, 0x43c5, 0xaf, 0x42, 0x4c, 0x0c, 0x7a, 0x0b, 0x8a, 0xf1);

class CWarzoneApoMfx final : public CBaseAudioProcessingObject, public IAudioSystemEffects {
public:
    CWarzoneApoMfx();
    ~CWarzoneApoMfx() override = default;

    STDMETHODIMP_(void) APOProcess(UINT32 numInputConnections,
                                   APO_CONNECTION_PROPERTY** inputConnections,
                                   UINT32 numOutputConnections,
                                   APO_CONNECTION_PROPERTY** outputConnections) override;

    STDMETHODIMP GetLatency(HNSTIME* latency) override;
    STDMETHODIMP LockForProcess(UINT32 numInputConnections,
                                APO_CONNECTION_DESCRIPTOR** inputConnections,
                                UINT32 numOutputConnections,
                                APO_CONNECTION_DESCRIPTOR** outputConnections) override;
    STDMETHODIMP UnlockForProcess() override;

    STDMETHODIMP IsInputFormatSupported(IAudioMediaType* outputFormat,
                                        IAudioMediaType* requestedInputFormat,
                                        IAudioMediaType** supportedInputFormat) override;

    STDMETHODIMP GetEffectsList(GUID** effectsIds, UINT* effectCount, HANDLE event) override;

    void SetParamsFromControlBridge(const warzone_audio::EngineParams& params) noexcept;
    warzone_audio::RealtimeSnapshot GetSnapshotForControlBridge() const noexcept;

private:
    bool supportsFormat(const WAVEFORMATEX* format) const noexcept;
    UINT32 samplesPerFrame() const noexcept { return channels_; }

    warzone_audio::RealtimeEngine realtime_;
    UINT32 channels_ = 2;
    UINT32 maxFrames_ = 512;
    bool locked_ = false;
};

#endif // WARZONE_APO_WDK_BUILD
