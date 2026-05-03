//! Mini Radar Overlay — circular floating minimap for 7.1 spatial audio.
//! Renders a military-style radar with sweep line, concentric rings,
//! crosshair, and directional audio blips as a Win32 layered window.

use std::f32::consts::PI;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HBRUSH, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::models::RadarSnapshotDto;

// ── Dimensions ──
const RADAR_SIZE: i32 = 220;
const RADAR_HALF: f32 = RADAR_SIZE as f32 / 2.0;
const MARGIN: i32 = 40;
const FRAME_TIME: Duration = Duration::from_millis(33); // ~30fps
const SWEEP_SPEED: f32 = 2.0 * PI / 3.0; // full rotation in 3 seconds

// ── Position constants ──
pub const POS_TOP_LEFT: u8 = 0;
pub const POS_TOP_RIGHT: u8 = 1;
pub const POS_BOTTOM_LEFT: u8 = 2;
pub const POS_BOTTOM_RIGHT: u8 = 3;

const CLASS_NAME: PCWSTR = w!("VanySoundMiniRadar");

// ── Colors (BGRA pre-multiplied) ──
const BG_COLOR: [u8; 4] = [14, 14, 10, 200]; // dark bg
const RING_COLOR: [u8; 3] = [0x30, 0x90, 0x18]; // green rings
const CROSS_COLOR: [u8; 3] = [0x28, 0x70, 0x14]; // crosshair
const SWEEP_COLOR: [u8; 3] = [0x40, 0xFF, 0x30]; // sweep line
const BORDER_COLOR: [u8; 3] = [0x30, 0xCC, 0x20]; // outer ring
const SWEEP_TRAIL: [u8; 3] = [0x20, 0xA0, 0x10]; // trail behind sweep

/// 7.1 channel positions on the radar (angle in radians from top, clockwise)
/// Matches SpatialRadar.tsx CHANNEL_MAP layout
struct BlipConfig {
    angle_rad: f32,
    color: [u8; 3],
}

fn channel_blips() -> [BlipConfig; 5] {
    [
        // far_left → 225° (back-left, ~7:30 position)
        BlipConfig {
            angle_rad: 225.0 * PI / 180.0,
            color: [0xFF, 0xBF, 0x12],
        },
        // left → 280° (front-left, ~10 o'clock)
        BlipConfig {
            angle_rad: 280.0 * PI / 180.0,
            color: [0xFF, 0xE6, 0x14],
        },
        // center → 0° (front/top, 12 o'clock)
        BlipConfig {
            angle_rad: 0.0,
            color: [0xFF, 0xFF, 0x8E],
        },
        // right → 80° (front-right, ~2 o'clock)
        BlipConfig {
            angle_rad: 80.0 * PI / 180.0,
            color: [0xFF, 0xB2, 0x1C],
        },
        // far_right → 135° (back-right, ~4:30 position)
        BlipConfig {
            angle_rad: 135.0 * PI / 180.0,
            color: [0xFF, 0x8B, 0x1A],
        },
    ]
}

/// Spawns the mini radar overlay thread.
pub fn spawn_mini_radar(
    snapshot: Arc<RwLock<RadarSnapshotDto>>,
    enabled: Arc<AtomicBool>,
    position: Arc<AtomicU8>,
) {
    thread::Builder::new()
        .name("mini-radar".to_string())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tracing::info!("mini-radar thread running");
                unsafe {
                    use windows::Win32::UI::HiDpi::*;
                    let _ =
                        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                }
                match run_mini_radar(snapshot, enabled, position) {
                    Ok(()) => tracing::info!("mini-radar exited normally"),
                    Err(e) => tracing::error!("mini-radar error: {:#}", e),
                }
            }));
            if let Err(panic) = result {
                let msg = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "unknown panic".into());
                tracing::error!("mini-radar PANIC: {}", msg);
            }
        })
        .expect("Failed to spawn mini-radar thread");
    tracing::info!("Mini radar thread spawned");
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn register_class() -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_OWNDC,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hbrBackground: HBRUSH(null_mut()),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            let err = windows::Win32::Foundation::GetLastError();
            // Class already registered is OK
            if err.0 != 1410 {
                return Err(anyhow!("RegisterClassExW failed: {:?}", err));
            }
        }
    }
    Ok(())
}

/// Compute window (x, y) from position code and screen dimensions.
fn compute_position(pos: u8, screen_w: i32, screen_h: i32) -> (i32, i32) {
    match pos {
        POS_TOP_LEFT => (MARGIN, MARGIN),
        POS_TOP_RIGHT => (screen_w - RADAR_SIZE - MARGIN, MARGIN),
        POS_BOTTOM_LEFT => (MARGIN, screen_h - RADAR_SIZE - MARGIN),
        _ => (
            screen_w - RADAR_SIZE - MARGIN,
            screen_h - RADAR_SIZE - MARGIN,
        ), // BR default
    }
}

fn run_mini_radar(
    snapshot: Arc<RwLock<RadarSnapshotDto>>,
    enabled: Arc<AtomicBool>,
    position: Arc<AtomicU8>,
) -> Result<()> {
    register_class()?;

    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    tracing::info!("Mini radar screen: {}x{}", screen_w, screen_h);

    let initial_pos = position.load(Ordering::Relaxed);
    let (init_x, init_y) = compute_position(initial_pos, screen_w, screen_h);

    // Create layered window
    let instance = unsafe { GetModuleHandleW(None) }?;
    let ex_style =
        WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            CLASS_NAME,
            w!("VanySound Mini Radar"),
            WS_POPUP,
            init_x,
            init_y,
            RADAR_SIZE,
            RADAR_SIZE,
            None,
            None,
            instance,
            None,
        )
    }?;

    // Pre-allocate GDI resources
    let screen_dc_init = unsafe { GetDC(HWND::default()) };
    let mem_dc = unsafe { CreateCompatibleDC(screen_dc_init) };
    unsafe {
        ReleaseDC(HWND::default(), screen_dc_init);
    }
    if mem_dc.0.is_null() {
        return Err(anyhow!("CreateCompatibleDC failed"));
    }

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: RADAR_SIZE,
        biHeight: -RADAR_SIZE,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let mut bits_ptr: *mut c_void = null_mut();
    let bitmap = unsafe { CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0) }?;
    let previous_obj = unsafe { SelectObject(mem_dc, HGDIOBJ(bitmap.0)) };
    let pixels_ptr = bits_ptr.cast::<u8>();

    // Pixel buffer
    let buf_len = (RADAR_SIZE * RADAR_SIZE * 4) as usize;
    let mut pixels = vec![0u8; buf_len];
    let blips = channel_blips();

    let mut sweep_angle: f32 = 0.0;
    let mut was_visible = false;
    let mut last_pos = initial_pos;
    let mut current_x = init_x;
    let mut current_y = init_y;
    let mut topmost_counter: u32 = 0;

    tracing::info!("Mini radar entering render loop");

    loop {
        let is_enabled = enabled.load(Ordering::Relaxed);

        // Handle visibility transitions
        if is_enabled && !was_visible {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            was_visible = true;
        } else if !is_enabled && was_visible {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            was_visible = false;
        }

        if !is_enabled {
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        // Handle position changes
        let cur_pos = position.load(Ordering::Relaxed);
        if cur_pos != last_pos {
            let (nx, ny) = compute_position(cur_pos, screen_w, screen_h);
            current_x = nx;
            current_y = ny;
            last_pos = cur_pos;
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    nx,
                    ny,
                    RADAR_SIZE,
                    RADAR_SIZE,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }

        // Re-assert topmost periodically
        topmost_counter += 1;
        if topmost_counter % 90 == 0 {
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    current_x,
                    current_y,
                    RADAR_SIZE,
                    RADAR_SIZE,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }

        // Advance sweep
        sweep_angle += SWEEP_SPEED * (FRAME_TIME.as_secs_f32());
        if sweep_angle >= 2.0 * PI {
            sweep_angle -= 2.0 * PI;
        }

        // Read audio snapshot
        let snap = snapshot.read().map(|g| g.clone()).unwrap_or_default();
        let channel_levels = [
            snap.far_left,
            snap.left,
            snap.center,
            snap.right,
            snap.far_right,
        ];

        // ── Render ──
        render_frame(
            &mut pixels,
            sweep_angle,
            &blips,
            &channel_levels,
            snap.ambience,
        );

        // Blit to screen
        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), pixels_ptr, buf_len);
        }

        let screen_dc = unsafe { GetDC(HWND::default()) };
        if !screen_dc.0.is_null() {
            let dst = POINT {
                x: current_x,
                y: current_y,
            };
            let src = POINT { x: 0, y: 0 };
            let size = SIZE {
                cx: RADAR_SIZE,
                cy: RADAR_SIZE,
            };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            unsafe {
                let _ = UpdateLayeredWindow(
                    hwnd,
                    screen_dc,
                    Some(&dst),
                    Some(&size),
                    mem_dc,
                    Some(&src),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                ReleaseDC(HWND::default(), screen_dc);
            }
        }

        thread::sleep(FRAME_TIME);
    }
}

// ═══════════════════════════════════════════════════════════════════
//  RENDERING — all pixel operations for the circular radar
// ═══════════════════════════════════════════════════════════════════

fn render_frame(
    pixels: &mut [u8],
    sweep_angle: f32,
    blips: &[BlipConfig; 5],
    levels: &[f32; 5],
    ambience: f32,
) {
    pixels.fill(0);
    let size = RADAR_SIZE as usize;

    draw_background(pixels, size);
    draw_rings(pixels, size);
    draw_crosshair(pixels, size);
    draw_sweep(pixels, size, sweep_angle);
    draw_blips(pixels, size, blips, levels, ambience);
    draw_border(pixels, size);
}

/// Dark circular background with alpha
fn draw_background(pixels: &mut [u8], size: usize) {
    let half = size as f32 / 2.0;
    let r_outer = half - 2.0;
    let r_outer_sq = r_outer * r_outer;

    for y in 0..size {
        let dy = y as f32 - half;
        let dy2 = dy * dy;
        for x in 0..size {
            let dx = x as f32 - half;
            let dist_sq = dx * dx + dy2;
            if dist_sq > r_outer_sq {
                continue;
            }

            let idx = (y * size + x) * 4;

            // Slight radial gradient — darker at edges
            let t = (dist_sq / r_outer_sq).sqrt();
            let alpha_factor = 1.0 - t * 0.15;
            let a = (BG_COLOR[3] as f32 * alpha_factor) as u8;

            // Pre-multiply
            let r = ((BG_COLOR[2] as u16 * a as u16) / 255) as u8;
            let g = ((BG_COLOR[1] as u16 * a as u16) / 255) as u8;
            let b = ((BG_COLOR[0] as u16 * a as u16) / 255) as u8;

            pixels[idx] = b;
            pixels[idx + 1] = g;
            pixels[idx + 2] = r;
            pixels[idx + 3] = a;
        }
    }
}

/// Concentric range rings
fn draw_rings(pixels: &mut [u8], size: usize) {
    let half = size as f32 / 2.0;
    let ring_radii = [half * 0.25, half * 0.5, half * 0.75];
    let ring_alpha: f32 = 0.25;

    for &radius in &ring_radii {
        draw_circle_outline(
            pixels, size, half, half, radius, RING_COLOR, ring_alpha, 1.2,
        );
    }
}

/// Crosshair lines (vertical + horizontal through center)
fn draw_crosshair(pixels: &mut [u8], size: usize) {
    let half = size as f32 / 2.0;
    let r_inner = half - 4.0;
    let alpha: f32 = 0.18;
    let center = (size / 2) as i32;

    // Horizontal line
    for x in 0..size {
        let dx = (x as f32 - half).abs();
        if dx > r_inner {
            continue;
        }
        blend_pixel(pixels, size, x, center as usize, CROSS_COLOR, alpha);
    }
    // Vertical line
    for y in 0..size {
        let dy = (y as f32 - half).abs();
        if dy > r_inner {
            continue;
        }
        blend_pixel(pixels, size, center as usize, y, CROSS_COLOR, alpha);
    }
}

/// Rotating sweep line with trailing fade
fn draw_sweep(pixels: &mut [u8], size: usize, angle: f32) {
    let half = size as f32 / 2.0;
    let r_max = half - 4.0;

    // Draw trail (fading arc behind sweep)
    let trail_arc = 0.65; // radians of trail
    let trail_steps = 30;
    for step in 0..trail_steps {
        let t = step as f32 / trail_steps as f32;
        let trail_angle = angle - t * trail_arc;
        let trail_alpha = (1.0 - t) * 0.12;
        if trail_alpha < 0.005 {
            continue;
        }

        let cos_a = (trail_angle - PI / 2.0).cos();
        let sin_a = (trail_angle - PI / 2.0).sin();

        for r in 4..(r_max as i32) {
            let rf = r as f32;
            let px = half + cos_a * rf;
            let py = half + sin_a * rf;
            let ix = px as usize;
            let iy = py as usize;
            if ix < size && iy < size {
                blend_pixel(pixels, size, ix, iy, SWEEP_TRAIL, trail_alpha);
            }
        }
    }

    // Draw main sweep line
    let cos_a = (angle - PI / 2.0).cos();
    let sin_a = (angle - PI / 2.0).sin();

    for r in 2..(r_max as i32) {
        let rf = r as f32;
        let px = half + cos_a * rf;
        let py = half + sin_a * rf;

        // Anti-aliased: draw 2px wide
        for dy in -1i32..=0 {
            for dx in -1i32..=0 {
                let fx = (px as i32 + dx) as usize;
                let fy = (py as i32 + dy) as usize;
                if fx < size && fy < size {
                    // Intensity increases toward the tip
                    let intensity = 0.3 + (rf / r_max) * 0.5;
                    blend_pixel(pixels, size, fx, fy, SWEEP_COLOR, intensity);
                }
            }
        }
    }

    // Bright dot at center
    draw_filled_circle(pixels, size, half, half, 3.0, SWEEP_COLOR, 0.8);
}

/// Directional audio blips
fn draw_blips(
    pixels: &mut [u8],
    size: usize,
    blips: &[BlipConfig; 5],
    levels: &[f32; 5],
    ambience: f32,
) {
    let half = size as f32 / 2.0;

    for (i, blip) in blips.iter().enumerate() {
        let raw_level = levels[i];
        if raw_level < 0.008 {
            continue;
        }

        let level = soft_ceil(raw_level);
        // Distance from center: louder → further out, min 20% radius
        let dist = half * (0.20 + level.min(1.0) * 0.55);

        // Convert angle to x,y (angle from top, clockwise)
        let cos_a = (blip.angle_rad - PI / 2.0).cos();
        let sin_a = (blip.angle_rad - PI / 2.0).sin();
        let bx = half + cos_a * dist;
        let by = half + sin_a * dist;

        // Outer glow
        let glow_radius = 8.0 + level * 12.0;
        let glow_alpha = level * 0.35;
        draw_filled_circle(pixels, size, bx, by, glow_radius, blip.color, glow_alpha);

        // Core dot
        let core_radius = 2.5 + level * 3.5;
        let core_alpha = 0.5 + level * 0.45;
        draw_filled_circle(pixels, size, bx, by, core_radius, blip.color, core_alpha);

        // Hot center
        draw_filled_circle(pixels, size, bx, by, 1.8, [0xFF, 0xFF, 0xFF], level * 0.6);
    }

    // Ambient ring — subtle glow around center when ambience is present
    if ambience > 0.01 {
        let amb_level = soft_ceil(ambience);
        let amb_radius = half * 0.15 + amb_level * half * 0.12;
        draw_circle_outline(
            pixels,
            size,
            half,
            half,
            amb_radius,
            [0x40, 0xCC, 0x30],
            amb_level * 0.3,
            2.0,
        );
    }
}

/// Outer border ring
fn draw_border(pixels: &mut [u8], size: usize) {
    let half = size as f32 / 2.0;
    let r_outer = half - 2.0;
    draw_circle_outline(pixels, size, half, half, r_outer, BORDER_COLOR, 0.55, 1.8);
    draw_circle_outline(
        pixels,
        size,
        half,
        half,
        r_outer - 1.5,
        BORDER_COLOR,
        0.20,
        1.0,
    );
}

// ═══════════════════════════════════════════════════════════════════
//  PIXEL PRIMITIVES
// ═══════════════════════════════════════════════════════════════════

/// Blend a single pixel (pre-multiplied alpha compositing over existing content)
fn blend_pixel(pixels: &mut [u8], size: usize, x: usize, y: usize, color: [u8; 3], alpha: f32) {
    if x >= size || y >= size {
        return;
    }
    let idx = (y * size + x) * 4;
    if idx + 3 >= pixels.len() {
        return;
    }

    let a = (alpha * 255.0).min(255.0) as u16;
    let inv_a = 255 - a;

    // color is [B, G, R] in our convention
    let src_b = ((color[0] as u16 * a) / 255) as u8;
    let src_g = ((color[1] as u16 * a) / 255) as u8;
    let src_r = ((color[2] as u16 * a) / 255) as u8;

    pixels[idx] = ((pixels[idx] as u16 * inv_a / 255) + src_b as u16).min(255) as u8;
    pixels[idx + 1] = ((pixels[idx + 1] as u16 * inv_a / 255) + src_g as u16).min(255) as u8;
    pixels[idx + 2] = ((pixels[idx + 2] as u16 * inv_a / 255) + src_r as u16).min(255) as u8;
    pixels[idx + 3] = ((pixels[idx + 3] as u16 * inv_a / 255) + a).min(255) as u8;
}

/// Draw a filled circle with alpha
fn draw_filled_circle(
    pixels: &mut [u8],
    size: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    color: [u8; 3],
    alpha: f32,
) {
    let r2 = radius * radius;
    let x_min = ((cx - radius).floor() as i32).max(0) as usize;
    let x_max = ((cx + radius).ceil() as i32).min(size as i32 - 1) as usize;
    let y_min = ((cy - radius).floor() as i32).max(0) as usize;
    let y_max = ((cy + radius).ceil() as i32).min(size as i32 - 1) as usize;

    for y in y_min..=y_max {
        let dy = y as f32 - cy;
        let dy2 = dy * dy;
        for x in x_min..=x_max {
            let dx = x as f32 - cx;
            let d2 = dx * dx + dy2;
            if d2 > r2 {
                continue;
            }

            // Soft edge anti-aliasing
            let edge = 1.0 - ((d2.sqrt() - radius + 1.5) / 1.5).clamp(0.0, 1.0);
            let a = alpha * edge;
            if a < 0.004 {
                continue;
            }

            blend_pixel(pixels, size, x, y, color, a);
        }
    }
}

/// Draw a circle outline with given thickness
fn draw_circle_outline(
    pixels: &mut [u8],
    size: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    color: [u8; 3],
    alpha: f32,
    thickness: f32,
) {
    let inner = radius - thickness / 2.0;
    let outer = radius + thickness / 2.0;
    let inner2 = inner * inner;
    let outer2 = outer * outer;

    let x_min = ((cx - outer).floor() as i32).max(0) as usize;
    let x_max = ((cx + outer).ceil() as i32).min(size as i32 - 1) as usize;
    let y_min = ((cy - outer).floor() as i32).max(0) as usize;
    let y_max = ((cy + outer).ceil() as i32).min(size as i32 - 1) as usize;

    for y in y_min..=y_max {
        let dy = y as f32 - cy;
        let dy2 = dy * dy;
        for x in x_min..=x_max {
            let dx = x as f32 - cx;
            let d2 = dx * dx + dy2;
            if d2 < inner2 || d2 > outer2 {
                continue;
            }

            // Anti-alias both edges
            let dist = d2.sqrt();
            let outer_aa = 1.0 - ((dist - outer + 1.0) / 1.0).clamp(0.0, 1.0);
            let inner_aa = ((dist - inner) / 1.0).clamp(0.0, 1.0);
            let a = alpha * outer_aa * inner_aa;
            if a < 0.004 {
                continue;
            }

            blend_pixel(pixels, size, x, y, color, a);
        }
    }
}

/// Soft ceiling — matches overlay.rs/SpatialRadar.tsx
fn soft_ceil(x: f32) -> f32 {
    const SOFT_KNEE: f32 = 0.28;
    const SOFT_MAX: f32 = 0.55;
    if x <= SOFT_KNEE {
        return x;
    }
    let t = (x - SOFT_KNEE) / (1.0 - SOFT_KNEE);
    SOFT_KNEE + (SOFT_MAX - SOFT_KNEE) * t.powf(0.45)
}
