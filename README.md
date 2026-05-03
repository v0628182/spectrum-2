# Spectrum 2

VanySound/Spectrum 2 para Windows: app Tauri con motor DSP nativo FirstEdition, replay WASAPI cable -> audifonos y controles avanzados de procesamiento.

El procesamiento se aplica al flujo capturado del cable antes de reproducirse en los audifonos. No instala el DSP sobre la salida global de los audifonos, asi que YouTube/Discord pueden seguir sonando normal si salen directo a los audifonos.

## App principal

```powershell
cd app
npm install
npm run build
npm run tauri:build
```

La app incluye:

- DSP Engine realtime con el core FirstEdition.
- Spectrum Analyzer de 32 bandas leyendo el audio real del cable.
- Preset `Mejor OPC` y presets personalizados con nombre.
- Preset `Solo Arma - Resto Nativo` para reducir el arma sin bajar todo el juego.
- Controles avanzados: nivel de cambio, sutileza, wet/dry, frecuencias, Q por banda, ataques/releases, limiter release y stereo width.
- Reduccion de disparos por bandas de arma, con modo mid/side que evita procesar toda la mezcla.
- Ruta de baja latencia con paquetes de 128 frames y render WASAPI de periodo minimo cuando el driver lo permite.

El empaquetado puede fallar al final si no existe `TAURI_SIGNING_PRIVATE_KEY`; el EXE/MSI/setup se generan antes del paso de firma.

## Release

El paquete transferible actualizado esta en:

```text
release/VanySound_Setup_TRANSFER_1.0.12.zip
```

## Core DSP legacy

## Estructura

```text
include/warzone_audio/  API y tipos publicos
src/                    Motor DSP, detector y procesador
tools/                  Pruebas sinteticas
build.ps1               Build local con g++ o clang++
```

## Build

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build.ps1
```

## Prueba sintetica

```powershell
.\build\synthetic_test.exe
```

El test genera ambiente, pasos sinteticos, explosion broadband/low-end y mezcla de eventos. Valida que el detector separe pasos/proteccion y que el limiter mantenga el techo.

## Benchmark

```powershell
.\build\benchmark.exe
```

## Simulacion real-time

```powershell
.\build\realtime_sim.exe config\warzone_reference_v1.ini
```

Esta prueba procesa bloques de `64/128/256/512` frames, cambia parametros durante el procesamiento y valida que el adapter `RealtimeEngine` no haga bypass inesperado ni genere muestras invalidas.

## Procesar WAV

```powershell
.\build\wav_process.exe input.wav output.wav config\default_settings.ini logs\clip.csv
```

La herramienta offline acepta WAV `48 kHz` mono/estereo en `PCM16` o `float32`, procesa con el mismo core DSP y escribe un WAV estereo `PCM16`.

Para generar un fixture de prueba:

```powershell
.\build\generate_fixture_wav.exe .\build\fixture_input.wav
.\build\wav_process.exe .\build\fixture_input.wav .\build\fixture_output.wav config\default_settings.ini logs\fixture.csv
```

## Artefactos

```text
build/warzone_audio_core.dll
build/synthetic_test.exe
build/benchmark.exe
build/realtime_sim.exe
build/wav_process.exe
```

## Config

La configuracion inicial esta en:

```text
config/default_settings.ini
```

La UI de Tauri debe controlar estos parametros por IPC/config, pero el hilo de audio no debe depender de Tauri.

## Custom APO

La ruta final sin VST2 esta documentada en:

```text
APO_RUNTIME_ARCHITECTURE.md
apo/
```

El scaffold del APO necesita Visual Studio + WDK. El build portable de este repo valida el core DSP y el adapter real-time, pero no instala un APO firmado.

## Presets y lotes

El preset base calibrado esta en:

```text
config/competitive_default.ini
```

Para procesar una carpeta completa de WAV:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\process_folder.ps1 -InputDir "C:\path\clips" -Config config\competitive_default.ini
```

## Interfaz visual de calibracion

```powershell
node .\tools\calibration_server.js
```

Abrir:

```text
http://localhost:4177
```

## Validar clips anotados

La UI puede guardar marcas manuales en:

```text
captures/annotations/*.markers.json
```

Validar un clip anotado contra el preset baseline:

```powershell
.\build\validate_annotations.exe .\captures\annotations\aa.3ini.markers.json .\config\warzone_reference_v1.ini .\captures\validation\aa.warzone_reference_v1.validation.json
```

El validador revisa ventanas cercanas a las marcas:

```text
footstep: +/-350 ms, acepta footstep >= 0.60
gunshot/airstrike: +/-500 ms, acepta protection >= 0.70
```

Un fallo de validacion no significa que el audio no sirva; significa que el detector todavia no esta midiendo ese evento con suficiente margen para produccion.
