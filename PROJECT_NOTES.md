# Spectrum 2

Proyecto VanySound/Spectrum con:

- App Tauri/React en `app/`.
- Motor DSP FirstEdition integrado en `app/src-tauri/native/warzone_audio/`.
- DSP Engine realtime aplicado solo al flujo capturado del cable antes de reproducirse en audifonos.
- Preset `Mejor OPC`.
- Guardado, carga y borrado de presets personalizados desde DSP Engine Control.
- Spectrum Analyzer realtime de 32 bandas basado en el audio capturado del cable.
- Ruta low-latency con paquetes de 128 frames, buffer de render minimo WASAPI cuando el driver lo permite y cola corta que descarta audio viejo en vez de acumular delay.
- Controles DSP extra activos: output trim, residual reduction, balance low/mid/high, STFT cutoff/preserve, transient kill, spectral mask y protection pasos.
- Paquete instalador actualizado en `release/VanySound_Setup_TRANSFER_1.0.12.zip`.

## Build

```powershell
cd app
npm install
npm run build
npm run tauri:build
```

El empaquetado puede fallar al final si no existe `TAURI_SIGNING_PRIVATE_KEY`; el ejecutable y los instaladores se generan antes del paso de firma del updater.
