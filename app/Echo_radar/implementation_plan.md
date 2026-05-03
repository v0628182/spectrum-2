# EchoAudio Radar — Plan Maestro (Solo Overlay Visual)

## Estado actual del código

- `src/main.rs` — Entry point, lanza threads, configura ventana eframe
- `src/audio.rs` — WASAPI loopback capture, envía `(Vec<f32>, u16)` por channel
- `src/analysis.rs` — RMS pan detection, EMA smoothing, unit tests OK
- `src/overlay.rs` — eframe App, pinta glow en bordes — **PROBLEMAS: pantalla negra**
- `Cargo.toml` — eframe 0.31 con backend `glow` (OpenGL)

---

## Problema raíz: Pantalla negra

El backend `glow` de eframe renderiza en un contexto OpenGL. Cuando se pide
`with_transparent(true)` en eframe 0.31, el compositor de Windows (DWM) necesita
recibir alpha = 0.0 en los píxeles del background para que sean transparentes.
El problema es que eframe/glow no siempre propaga el alpha correctamente al
backbuffer. La solución está en la configuración de la ventana Win32 + DWM.

---

## Solución: Ventanas delgadas en los bordes (no fullscreen)

En vez de una sola ventana fullscreen, crear **4 ventanas delgadas** pegadas
a cada borde del monitor:

```
┌─[L 80px]────────────────[R 80px]─┐
│                                   │
[T 60px]    (espacio del juego)  [B (no)]
│                                   │
└───────────────────────────────────┘
```

Ventana Left:   x=0,      y=0,  w=80,   h=screen_h
Ventana Right:  x=sw-80,  y=0,  w=80,   h=screen_h
Ventana Top:    x=80,     y=0,  w=sw-160, h=60

Cada ventana es:
- Sin decoraciones (`WS_POPUP`)
- Siempre encima (`WS_EX_TOPMOST`)
- Click-through (`WS_EX_TRANSPARENT`)
- Layered con alpha (`WS_EX_LAYERED`)
- Sin barra de tareas (`WS_EX_TOOLWINDOW`)
- Fondo transparente real via `UpdateLayeredWindow` o `SetLayeredWindowAttributes`

---

## Arquitectura definitiva (sin eframe)

Abandonar eframe para el overlay. Usar Win32 puro con `windows` crate:
**es más control, menos problemas de transparencia, menos dependencias.**

```
main.rs
├── audio::capture_thread   → crossbeam_channel → (Vec<f32>, u16)
├── analysis::analyze()     → AudioState { pan, volume }
└── overlay::run()          → Win32 window loop
    ├── create_edge_windows()   → HWND x4
    ├── paint_loop()            → GDI+ o Direct2D gradient rects
    └── recv_audio() + repaint  → ~60fps timer
```

---

## Archivo: `src/overlay.rs` — Reescritura completa

### Structs necesarios

```rust
pub struct EdgeWindows {
    pub left:  HWND,
    pub right: HWND,
    pub top:   HWND,
}

pub struct GlowState {
    pub left_alpha:  f32,   // 0.0..1.0
    pub right_alpha: f32,
    pub top_alpha:   f32,
    pub hue: f32,           // 195.0 = cyan default
}
```

### Función: `create_edge_window(x, y, w, h, title) -> HWND`

Pasos exactos en orden:
1. `RegisterClassExW` con `hbrBackground = GetStockObject(NULL_BRUSH)` — CRÍTICO para transparencia
2. `CreateWindowExW` con flags:
   - dwExStyle: `WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`
   - dwStyle: `WS_POPUP` (sin borde, sin titlebar)
3. `SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA)` — alpha total del render
4. `ShowWindow(hwnd, SW_SHOWNOACTIVATE)` — no robar focus al juego
5. Retornar HWND

### Función: `paint_left_glow(hwnd, intensity, hue)`

Usar `BeginPaint` / `EndPaint` con GDI:
1. `GetClientRect(hwnd)` → rect
2. `CreateCompatibleDC` + `CreateCompatibleBitmap` (double buffer)
3. `BitBlt` negro con alpha 0 como base
4. Loop de capas (20 iteraciones):
   - `t = i / 20.0`
   - `alpha_byte = (intensity * (1-t)^2 * 200) as u8`
   - Color cyan: R=0, G=200, B=255 con variación en `t`
   - `CreateSolidBrush(RGB(r,g,b))`
   - `FillRect` en la franja correspondiente
5. `BitBlt` buffer → DC real
6. `DeleteDC` / `DeleteObject` limpieza

> **Alternativa mejor**: usar `GdiGradientFill` con `TRIVERTEX` para el gradiente
> en una sola llamada en vez del loop. Más eficiente y suave.

### Función: `paint_right_glow(hwnd, intensity, hue)` — Simétrico al left

### Función: `paint_top_glow(hwnd, intensity, hue)` — Gradiente vertical

### Función: `message_loop(rx, windows, state)`

```
loop {
    // 1. Procesar mensajes Win32 pendientes (non-blocking)
    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
        if msg.message == WM_QUIT { return; }
    }

    // 2. Recibir audio del channel (non-blocking, drenar todos)
    while let Ok((samples, ch)) = rx.try_recv() {
        let new_state = analysis::analyze_stereo(&samples, ch);
        state = analysis::smooth(&new_state, &state, 0.35);
    }
    // Sin datos → decay
    if rx.is_empty() {
        state = analysis::smooth(&AudioState::default(), &state, 0.05);
    }

    // 3. Calcular intensidades L/R/T
    let pan = state.pan;
    let vol = (state.volume * 4.0).min(1.0);
    let li = (-pan).max(0.0) * vol + (1.0 - pan.abs()) * vol * 0.4;
    let ri =  pan.max(0.0)  * vol + (1.0 - pan.abs()) * vol * 0.4;
    let ti = vol * 0.25;

    // 4. Invalidar y repintar ventanas con intensidad
    InvalidateRect(windows.left,  None, false);
    InvalidateRect(windows.right, None, false);
    InvalidateRect(windows.top,   None, false);

    // 5. Sleep 16ms ≈ 60fps
    std::thread::sleep(Duration::from_millis(16));
}
```

### WndProc handler

```rust
unsafe extern "system" fn wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM
) -> LRESULT {
    match msg {
        WM_PAINT => {
            // Llamar paint_*_glow con los valores guardados en un AtomicF32 global
            // o en GWLP_USERDATA del HWND
        }
        WM_ERASEBKGND => {
            // Retornar 1 para prevenir borrado del fondo (evita flickering)
            LRESULT(1)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
```

---

## Archivo: `Cargo.toml` — Dependencias finales

```toml
[dependencies]
wasapi          = "0.23"
crossbeam-channel = "0.5"
anyhow          = "1"
tracing         = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
# Eliminar: eframe, egui (ya no se usan)

[dependencies.windows]
version = "0.58"
features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_Graphics_Gdi",
    "Win32_System_LibraryLoader",
]
```

---

## Archivo: `src/main.rs` — Entry point limpio

```rust
mod analysis;
mod audio;
mod overlay;

fn main() {
    // Init logging
    tracing_subscriber::fmt().init();

    // Channel audio → overlay
    let (tx, rx) = crossbeam_channel::bounded::<(Vec<f32>, u16)>(8);

    // Spawn audio capture thread
    let _handle = audio::start_capture(tx);

    // Bloquea en el message loop Win32
    overlay::run(rx);
}
```

---

## Archivo: `src/audio.rs` — Sin cambios necesarios

El archivo actual está correcto y compilando. Solo verificar:
- `initialize_mta().ok()` — correcto
- `get_default_device(&Direction::Render)` — correcto (loopback en Render device)
- `initialize_client(&format, &Direction::Capture, &mode)` — correcto

---

## Archivo: `src/analysis.rs` — Sin cambios necesarios

Los 6 unit tests pasan. La función `analyze_stereo` y `smooth` están correctos.

---

## Datos de pantalla correctos

```rust
// En overlay.rs al inicio de run()
let screen_w = GetSystemMetrics(SM_CXSCREEN);
let screen_h = GetSystemMetrics(SM_CYSCREEN);

// Dimensiones de ventanas borde
let edge_w = 80i32;   // ancho del glow lateral
let top_h  = 60i32;   // alto del glow superior

// Posiciones
// Left:  (0, 0, edge_w, screen_h)
// Right: (screen_w - edge_w, 0, edge_w, screen_h)
// Top:   (edge_w, 0, screen_w - edge_w*2, top_h)
```

---

## Detalles del color

Escala de colores por defecto (cyan/blue gaming):
- Base RGB cuando pan = izquierda: `#00C8FF` (R=0, G=200, B=255)
- Base RGB cuando pan = derecha:   `#00C8FF` (mismo, es simétrico)
- Capa más interna (pegada al borde): alpha máximo ~200/255
- Capa más externa (hacia el centro): alpha = 0

Fórmula por capa `i` de `N_LAYERS = 20`:
```
t = i as f32 / N_LAYERS as f32
alpha = (intensity * (1.0 - t).powi(2) * 200.0) as u8
b_val = 255u8
g_val = (200.0 - t * 60.0) as u8   // va de 200 a 140
r_val = (t * 30.0) as u8            // ligero toque cálido al exterior
```

---

## Pasos de verificación manual (checklist)

1. Compilar: `cargo build --release` → debe terminar sin `error[`
2. Ejecutar: `.\target\release\echo-audio.exe`
3. El desktop debe verse IGUAL que sin el exe (ventanas transparentes)
4. Abrir YouTube → buscar "Left Right Stereo Audio Test"
5. Al reproducir audio izquierdo → borde IZQUIERDO del monitor brilla cyan
6. Al reproducir audio derecho  → borde DERECHO brilla
7. Audio central  → ambos bordes brillan suave y simétrico
8. Hacer click sobre cualquier ventana bajo el overlay → click debe pasar
9. Abrir Task Manager → echo-audio.exe: CPU < 3%, RAM < 40 MB

---

## Errores comunes a evitar

| Error | Causa | Solución |
|-------|-------|----------|
| Pantalla negra | eframe wgpu/glow no propaga alpha | Usar Win32 puro con WS_EX_LAYERED |
| Overlay bloquea clicks | Falta WS_EX_TRANSPARENT | Agregar ese flag al dwExStyle |
| Aparece en taskbar | Falta WS_EX_TOOLWINDOW | Agregar ese flag |
| Flickering | WndProc no maneja WM_ERASEBKGND | Retornar LRESULT(1) en WM_ERASEBKGND |
| Roba focus al juego | ShowWindow con SW_SHOW normal | Usar SW_SHOWNOACTIVATE + WS_EX_NOACTIVATE |
| initialize_mta falla | Se llama desde el UI thread | Llamar desde el thread de audio, no main |
| Loopback no detecta audio | Direction::Capture en Capture device | Debe ser Render device + Direction::Capture |
