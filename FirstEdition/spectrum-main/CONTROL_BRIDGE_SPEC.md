# Especificacion del puente Tauri -> DSP DLL

## Principio

Tauri controla parametros, pero no procesa audio. El audio debe seguir funcionando si la UI se cierra, se congela o tarda.

Para la ruta Custom APO, el punto de entrada en C++ es `warzone_audio::RealtimeEngine`. La UI nunca llama directo al motor DSP desde el hilo de audio; solo publica snapshots de parametros validados.

## Arquitectura

```text
Tauri UI
  |
  | comandos de usuario
  v
Control Bridge nativo
  |
  | parametros atomicos / shared memory / named pipe
  v
DSP DLL cargado por APO/host
  |
  v
Audio procesado
```

## Parametros publicos

Estos son los controles que la UI debe mostrar inicialmente:

```text
footstepEnhance       0..100
actionDetail          0..100
gunshotReduction      0..100
explosionReduction    0..100
detectionSensitivity  0..100
outputCeilingDb       -12.0..-0.5
stepBodyBoostDb       0..20
stepClarityBoostDb    0..24
stepLowBodyBoostDb    0..14
stepLowMidBoostDb     0..14
weaponMidCutDb        -48..0
weaponAirCutDb        -48..0
sustainedHoldMs       100..1600
masterDuckDb          -24..0
impactDuckDb          -40..0
footstepLevelerAmount 0..100
footstepTargetRmsDb   -36..-14
footstepMaxLiftDb     0..18
footstepLevelerSpeedMs 10..250
stabilityAmount       0..100
spectralFloorDb       -48..-18
stableReleaseMs       80..500
footstepGuardAmount   0..100
maxCutStepDb          3..24
protectionExtreme     true/false
debugLogging          true/false
```

## Persistencia

Archivo recomendado:

```text
config/default_settings.ini
```

La UI puede guardar ahi el ultimo estado. El DSP no debe leer este archivo en cada bloque de audio. Solo debe usarse al iniciar o cuando un hilo no-real-time aplique cambios.

## IPC recomendado

Nombres reservados en codigo:

```text
\\.\pipe\warzone_audio_control
Local\WarzoneAudioParams
```

### Fase 1

Usar archivo INI + reload controlado por la app/host.

Ventaja: simple.

Limitacion: no ideal para cambios suaves en vivo.

### Fase 2

Usar named pipe para comandos:

```json
{"type":"setParams","params":{"footstepEnhance":75,"gunshotReduction":90}}
{"type":"loadPreset","path":"config/warzone_reference_v1.ini"}
{"type":"requestStats"}
{"type":"enableDebug","enabled":true}
```

Ventaja: facil para Tauri/Rust.

### Fase 3

Usar shared memory para parametros vivos y named pipe para logs/comandos.

Ventaja: parametros atomicos muy rapidos y sin I/O de disco.

Esta es la ruta objetivo para produccion. El named pipe puede existir primero para acelerar integracion, pero el APO final debe poder consumir el ultimo bloque valido sin esperar a la UI.

## Reglas real-time

- No leer archivos en el hilo de audio.
- No escribir logs directamente desde el hilo de audio.
- No asignar memoria dinamica durante `processBlock`.
- No bloquear esperando respuesta de Tauri.
- No depender de WebView, JS ni runtime de UI para procesar audio.
- Si IPC falla, mantener los ultimos parametros validos.

## API C actual

El core expone:

```c
wza_create_engine
wza_destroy_engine
wza_reset_engine
wza_prepare_engine
wza_set_params
wza_process_stereo
wza_process_interleaved
wza_get_scores
```

`wza_prepare_engine` debe llamarse antes de procesar audio interleaved para reservar buffers. Si llega un bloque mas grande que lo preparado, `wza_process_interleaved` hace passthrough en vez de asignar memoria.

## Logs

El logger actual escribe CSV con:

```text
frame, footstep, action, protection, lateral, confidence, outputPeak,
energyStepDb, energyLowMidDb, energyBassDb, snrStepDb,
superFluxStep, superFluxStepExcess, centroidHz, flatnessStep, crestDb
```

En produccion, el logger debe consumir una cola lock-free o snapshot no-real-time. La version actual sirve para herramientas/test, no para escribir desde callback de audio final.
