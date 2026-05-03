# EchoAudio - Plan de ejecucion propuesto

## Lo que realmente quieres construir

- Un radar de audio para Windows 10/11 escrito en Rust.
- Que convierta el audio estereo del juego en una señal visual util.
- Que si unos pasos, disparos o musica cargan mas al canal derecho, se vea del lado derecho.
- Que si cargan mas al canal izquierdo, se vea del lado izquierdo.
- Que si el sonido esta centrado, la señal visual tambien se vea equilibrada.
- Que funcione sin necesitar audifonos, usando el audio que sale por los speakers del sistema.
- Que el overlay sea discreto, transparente, click-through y suficientemente rapido para uso real en juego.

## Alcance correcto para la version 1

- Detectar direccion relativa `left / center / right`.
- Mostrar intensidad visual segun volumen y paneo.
- Funcionar con cualquier juego o app que salga por el dispositivo de audio por defecto.
- Capturar audio por WASAPI loopback sin integracion especial con el juego.

## Lo que esta fuera de alcance por ahora

- No es un radar 360 real.
- No puede saber posicion exacta en el mapa.
- No puede distinguir con certeza delante vs atras solo con audio estereo del sistema.
- No debe prometer mas de lo que la mezcla estereo realmente contiene.

La propuesta correcta es: tomar la informacion espacial que ya existe en la mezcla L/R y volverla visible.

## Estado actual del proyecto

- `src/audio.rs` ya implementa captura loopback con WASAPI y envia chunks por channel.
- `src/analysis.rs` ya calcula pan y volumen con una API simple y tiene tests unitarios.
- `src/overlay.rs` usa `eframe/egui` para dibujar un glow fullscreen.
- `src/main.rs` conecta captura, analisis visual y activacion inicial de Equalizer APO.
- `src/eq_control.rs` maneja perfiles de Equalizer APO y parece una pieza separada del overlay.
- `implementation_plan.md` ya identifica el principal problema tecnico: el overlay transparente actual probablemente falla por la combinacion `eframe + glow + transparencia`.

## Diagnostico principal

- El nucleo del producto ya existe: captura y analisis.
- El cuello de botella real es el overlay.
- Hoy hay una mezcla entre vision futura y codigo actual.
- El mayor riesgo es seguir agregando funciones sobre un overlay que todavia no es confiable.

## Objetivo inmediato recomendado

Entregar una version minima pero solida que haga una sola cosa muy bien:

- Abrir en Windows.
- Capturar audio del sistema.
- Detectar direccion L/R/C.
- Mostrar un radar visual en bordes reales, transparente y sin bloquear clicks.

Si eso queda estable, el proyecto ya tiene una base real para crecer.

## Plan de trabajo

### Fase 1 - Alinear el proyecto con una sola direccion

- Decidir oficialmente que el overlay deja de depender de `eframe/egui`.
- Mantener `audio.rs`, `analysis.rs` y `eq_control.rs` como piezas separadas.
- Tratar `implementation_plan.md` como base tecnica, pero actualizar la documentacion operativa para que describa el repo actual y no una arquitectura futura que aun no existe.

Resultado esperado:

- Una sola direccion tecnica clara: overlay Win32 puro.

### Fase 2 - Reescribir el overlay con Win32 puro

- Reemplazar `src/overlay.rs` por una implementacion con `windows` crate.
- Crear ventanas delgadas para left, right y top en lugar de una ventana fullscreen unica.
- Configurar estilos Win32 para `WS_POPUP`, `WS_EX_TOPMOST`, `WS_EX_TRANSPARENT`, `WS_EX_LAYERED`, `WS_EX_TOOLWINDOW` y `WS_EX_NOACTIVATE`.
- Pintar el glow con GDI en double buffer.
- Mantener un loop de mensajes Win32 que tambien consuma audio del channel.

Resultado esperado:

- Overlay visible sin pantalla negra.
- Click-through real.
- Sin barra de tareas.

### Fase 3 - Simplificar `main.rs` y dependencias

- Cambiar `main.rs` para que arranque audio, inicialice EQ opcional y entre al loop Win32.
- Quitar `eframe` y `egui` de `Cargo.toml` cuando el overlay nuevo compile.
- Agregar las features Win32 que falten para ventana, paint y carga de modulo.

Resultado esperado:

- El binario ya no depende del stack grafico actual.

### Fase 4 - Afinar comportamiento visual y latencia

- Ajustar ancho de glow lateral y alto de glow superior.
- Ajustar curvas de intensidad y smoothing para que responda rapido sin vibrar.
- Hacer decay suave cuando no llegan nuevos datos.
- Ajustar volumen minimo para evitar ruido visual permanente.

Resultado esperado:

- Sensacion de producto estable y premium, no de prototipo.

### Fase 5 - Verificacion real de uso

- Validar LEFT-only, RIGHT-only y CENTER.
- Validar transparencia real sobre escritorio y juego borderless.
- Validar que los clicks pasan a la app debajo del overlay.
- Medir uso de CPU y memoria.
- Confirmar que el proceso no roba foco.

Resultado esperado:

- Demo funcional que prueba el valor del producto.

## Lo que no haria todavia

- No dividiria el repo en multiples crates aun.
- No meteria licencia, activacion o anti-pirateria ahora.
- No trabajaria installer ni firma antes de tener un overlay estable.
- No agregaria mas UI o settings antes de resolver la transparencia real.

## Orden tecnico recomendado de archivos

1. `src/overlay.rs`
2. `src/main.rs`
3. `Cargo.toml`
4. Verificacion de que `src/audio.rs` y `src/analysis.rs` sigan acoplando bien
5. Ajustes opcionales en `src/eq_control.rs`

## Criterio de exito

- La app arranca sin pantalla negra.
- El overlay no tapa la pantalla ni intercepta clicks.
- Si los pasos o efectos estan cargados al canal izquierdo, el borde izquierdo reacciona claramente.
- Si los pasos o efectos estan cargados al canal derecho, el borde derecho reacciona claramente.
- Audio centrado produce respuesta equilibrada.
- La latencia visual es suficientemente baja para juego real.

## Mi lectura final

No creo que ahora quieras "mas funciones". Creo que quieres un producto muy concreto: ver la direccion del sonido sin depender de audifonos. La prioridad correcta es cerrar esa base tecnica primero y, solo despues, volver a EQ, settings, licencia o distribucion.
