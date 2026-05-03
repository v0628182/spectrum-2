# EchoAudio — Agent Rules

## Identidad del Proyecto
- **Nombre**: EchoAudio
- **Producto**: Radar de audio visual para videojuegos (overlay)
- **Lenguaje principal**: Rust (performance-critical core) + egui (UI overlay)
- **Target OS**: Windows 10/11 x64
- **Mercado**: ~60,000 gamers competitivos, licencia $75 USD

## Principios de Ingeniería

### 1. Performance es Ley
- El loop de captura de audio NUNCA debe bloquear el render del overlay.
- Latencia máxima aceptable: <15ms entre sonido y visualización.
- Usar threads dedicados: captura WASAPI → procesamiento DSP → render overlay.
- Zero allocations en el hot path. Pre-allocar buffers.
- El overlay **no debe** impactar FPS del juego (target: <1% GPU usage).

### 2. Arquitectura de Módulos
```
echo_audio/
├── crates/
│   ├── echo-core/        # Captura WASAPI + DSP (pan detection)
│   ├── echo-overlay/     # Ventana transparente + render egui
│   ├── echo-config/      # Persistencia de configuración (JSON/TOML)
│   └── echo-license/     # Validación de licencia (offline-first)
├── assets/               # Iconos, fuentes, shaders
├── installer/            # NSIS/WiX installer scripts
└── docs/                 # Documentación interna
```

### 3. Seguridad y Anti-Piratería
- La licencia se valida **offline-first** con clave RSA + hardware fingerprint.
- Online check sólo al activar. Funciona sin internet después.
- NO incluir secretos en el binario. Usar obfuscación ligera.
- Binario firmado con Microsoft Trusted Signing ($9.99/mo) para SmartScreen.

### 4. Distribución sin EV Certificate
- **Fase 1**: Microsoft Trusted Signing (Azure, $9.99/mo) — firma cloud.
- **Fase 2**: Submit a Microsoft para análisis de malware en cada release.
- **Fase 3**: Si no califica para Trusted Signing, firmar con OV cert (~$70/año) y construir reputación via descargas.
- **Fase 4**: Considerar Microsoft Store como canal de distribución adicional.
- Siempre proveer hash SHA-256 y instrucciones claras de "More Info → Run Anyway" en el sitio.

## Iteración y Testing

### Ciclo de Desarrollo
1. **Prototipar en etapas**: Audio captura → Análisis → Overlay básico → Overlay bonito → Licencia → Installer.
2. **Cada etapa debe tener un test verificable** antes de avanzar.
3. **Demo mode**: Siempre mantener un modo demo que genere audio sintético para testing sin necesidad de un juego corriendo.

### Tests Automatizados
- `cargo test` para unit tests de DSP (pan detection con señales conocidas).
- Tests de integración con archivos WAV pre-grabados (left-panned, right-panned, center).
- Benchmarks con `criterion` para medir latencia del pipeline.

### Tests Manuales (Checklist por Release)
1. ☐ Abrir EchoAudio → overlay aparece semitransparente sobre escritorio.
2. ☐ Reproducir video con audio LEFT-only → indicador visual aparece a la izquierda.
3. ☐ Reproducir video con audio RIGHT-only → indicador visual aparece a la derecha.
4. ☐ Reproducir audio CENTER → indicador visual centrado/simétrico.
5. ☐ Abrir un juego FPS en borderless windowed → overlay visible sobre el juego.
6. ☐ Verificar que clicks pasan a través del overlay (click-through).
7. ☐ Verificar uso de GPU <1% con Task Manager.
8. ☐ Cerrar/minimizar desde system tray funciona.
9. ☐ Licencia inválida → modo demo con marca de agua.
10. ☐ Licencia válida → funcionalidad completa sin marca de agua.

### Métricas de Calidad
- **Latencia**: Medir con `std::time::Instant` entre buffer recibido y frame renderizado.
- **CPU Usage**: El proceso EchoAudio no debe superar 2% CPU promedio.
- **Memoria**: <50MB RAM en operación normal.
- **Crashes**: Zero panics en producción. Usar `anyhow` + logging con `tracing`.

## Convenciones de Código
- Rust 2021 edition, `#![deny(clippy::all)]`.
- Nombres de variables/funciones en inglés. Comentarios en español OK.
- Cada crate tiene su propio `README.md`.
- Commits en formato Conventional Commits: `feat:`, `fix:`, `perf:`, `docs:`.
- Branch strategy: `main` (release), `develop` (integración), `feature/*`.

## Prioridades (en orden)
1. **Funciona**: El radar detecta correctamente dirección L/R/Center.
2. **Es rápido**: Latencia imperceptible.
3. **Es bonito**: El overlay se ve profesional y premium.
4. **Es seguro**: Licenciamiento robusto.
5. **Se distribuye**: Installer sin warnings de SmartScreen.
