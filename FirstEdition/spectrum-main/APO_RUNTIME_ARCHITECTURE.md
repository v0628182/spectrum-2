# Custom APO Runtime Architecture

## Decision

The production path is a Custom Windows APO over the virtual-cable endpoint controlled by the existing app. Tauri/UI remains outside the audio path and only controls parameters.

## Audio Path

```text
Warzone -> Virtual Cable -> Windows Audio Engine -> Warzone Custom APO -> Output device
```

The APO loads the DSP core and owns one `RealtimeEngine` instance per APO instance. The audio thread calls `RealtimeEngine::processInterleaved`, which uses preallocated buffers and atomic parameter snapshots.

## Control Path

```text
Tauri UI -> native control bridge -> validated EngineParams snapshot -> APO RealtimeEngine
```

Required commands:

```json
{"type":"setParams","params":{"footstepEnhance":100,"gunshotReduction":91}}
{"type":"loadPreset","path":"config/warzone_reference_v1.ini"}
{"type":"requestStats"}
{"type":"enableDebug","enabled":false}
```

The bridge may use named pipes first. For production, shared memory should hold the current parameter block and a revision counter; named pipes should remain for commands, preset loading and debug toggles.

## Real-Time Rules

- `APOProcess` must not allocate memory.
- `APOProcess` must not read INI/JSON files.
- `APOProcess` must not write logs.
- `APOProcess` must not call into Tauri, WebView or JavaScript.
- If parameters are invalid, keep the last valid snapshot.
- If a block is larger than the prepared size, bypass rather than allocate.
- If the format is unsupported, reject during `LockForProcess` or bypass safely.

## Format Policy

- Preferred format: `48 kHz`, `float32`, `1..8 channels`.
- Mono is duplicated to stereo internally.
- Stereo is processed natively.
- 5.1/7.1 is downmixed to stereo internally, processed, then returned into the first two output channels while extra channels are silenced.
- Any other sample rate is not a production target because the existing app owns the cable format.

## Current Implementation Status

- `RealtimeEngine` is the APO-ready adapter.
- `realtime_sim.exe` validates small blocks and live parameter changes without APO/WDK.
- `apo/` contains the WDK scaffold and INF template, but not a signed/installable driver package yet.
