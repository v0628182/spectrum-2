# Warzone Custom APO Scaffold

This folder is the production integration target. It is intentionally not part of
`build.ps1`, because a real APO must be built with Visual Studio + WDK and
registered with an INF package.

## Runtime Shape

```text
Game -> Virtual Cable endpoint -> Windows Audio Engine -> Warzone Custom APO -> Headphones
                                      ^
                                      |
                              UI / Tauri control bridge
```

The APO must only process audio in `APOProcess`. It must not read presets, write
logs, allocate heap memory, wait on IPC, or call UI code from the real-time path.

## Files

- `WarzoneApoSkeleton.h` / `WarzoneApoSkeleton.cpp`: WDK-facing APO class shape.
- `WarzoneApoDll.def`: COM export list expected by the APO DLL.
- `WarzoneApo.inf.template`: production INF starting point for Windows 10/11.

## Build Requirements

- Visual Studio with C++ desktop workload.
- Windows Driver Kit matching the target SDK.
- The existing `warzone_audio_core` sources or static library linked into the APO
  project.

## Control Model

The UI writes parameter snapshots through the control bridge. The APO consumes
only validated snapshots through `RealtimeEngine::setParams`; the audio thread
then applies the latest revision at the next block boundary.
