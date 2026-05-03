//! Fullscreen transparent overlay for 7.1 spatial radar visualization.
//! Adapted from standalone Echo_radar overlay — runs on its own Win32 thread.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
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

const EDGE_WIDTH: i32 = 112;
const TOP_HEIGHT: i32 = 148;
const FRAME_TIME: Duration = Duration::from_millis(16);
const SOFT_KNEE: f32 = 0.28;
const SOFT_MAX: f32 = 0.55;
const CLASS_NAME: PCWSTR = w!("VanySoundRadarOverlay");

#[derive(Debug, Clone, Copy, Default)]
struct OverlayState {
    far_left: f32,
    left: f32,
    center: f32,
    right: f32,
    far_right: f32,
    ambience: f32,
}

/// Spawn the overlay thread — reads from the shared RadarSnapshotDto.
/// `shutdown` flag is set by RadarService::Drop to signal exit.
pub fn spawn_overlay(
    snapshot: Arc<RwLock<RadarSnapshotDto>>,
    enabled: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    let join = thread::Builder::new()
        .name("radar-overlay".to_string())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tracing::info!("overlay thread running");
                overlay_log("overlay thread started");
                unsafe {
                    use windows::Win32::UI::HiDpi::*;
                    match SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
                    {
                        Ok(_) => tracing::info!("Overlay DPI awareness set OK"),
                        Err(e) => tracing::info!("Overlay DPI awareness already set: {:?}", e),
                    }
                }
                match run_overlay(snapshot, enabled, shutdown) {
                    Ok(()) => tracing::info!("overlay loop exited normally"),
                    Err(e) => {
                        tracing::error!("Overlay error: {:#}", e);
                        overlay_log(&format!("Overlay error: {:#}", e));
                    }
                }
            }));
            if let Err(panic) = result {
                let msg = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "unknown panic".into());
                tracing::error!("Overlay thread PANIC: {}", msg);
                overlay_log(&format!("Overlay thread PANIC: {}", msg));
            }
        })
        .expect("Failed to spawn overlay thread");
    tracing::info!("Overlay thread spawned");
    join
}

/// Synchronous log to a file — tracing non_blocking may not flush from secondary threads
fn overlay_log(msg: &str) {
    let path = crate::logging::log_root().join("overlay.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{}] {}", now, msg);
    }
}

fn run_overlay(
    snapshot: Arc<RwLock<RadarSnapshotDto>>,
    enabled: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    overlay_log("run_overlay: registering window class");
    if let Err(e) = register_window_class() {
        overlay_log(&format!(
            "register_window_class failed: {:#} — assuming already registered",
            e
        ));
    }

    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    tracing::info!(screen_w, screen_h, "Overlay screen metrics");
    if screen_w <= 0 || screen_h <= 0 {
        return Err(anyhow!("GetSystemMetrics returned invalid screen size"));
    }

    let top_width = (screen_w - EDGE_WIDTH * 2).max(320);

    tracing::info!("Overlay creating left window");
    let mut left = LayeredWindow::new(WindowRole::Left, 0, 0, EDGE_WIDTH, screen_h)?;
    tracing::info!("Overlay creating right window");
    let mut right = LayeredWindow::new(
        WindowRole::Right,
        screen_w - EDGE_WIDTH,
        0,
        EDGE_WIDTH,
        screen_h,
    )?;
    tracing::info!("Overlay creating top window");
    let mut top = LayeredWindow::new(WindowRole::Top, EDGE_WIDTH, 0, top_width, TOP_HEIGHT)?;
    tracing::info!("Overlay all windows created OK — entering render loop");

    let mut state = OverlayState::default();
    let mut was_visible = false;
    let mut log_counter: u32 = 0;
    let mut topmost_counter: u32 = 0;
    let mut present_errors: u32 = 0;

    loop {
        // ── Shutdown check: exit cleanly when app is closing ──
        if shutdown.load(Ordering::Relaxed) {
            tracing::info!("Overlay: shutdown signal received, exiting");
            break Ok(());
        }

        if process_messages() {
            return Ok(());
        }

        let is_enabled = enabled.load(Ordering::Relaxed);

        // Show/hide windows based on enabled state
        if is_enabled && !was_visible {
            tracing::info!("Overlay enabling visibility");
            left.set_visible(true);
            right.set_visible(true);
            top.set_visible(true);
            left.force_topmost();
            right.force_topmost();
            top.force_topmost();
            was_visible = true;
            topmost_counter = 0;
        } else if !is_enabled && was_visible {
            tracing::info!("Overlay disabling visibility");
            state = OverlayState::default();
            left.render(&state);
            right.render(&state);
            top.render(&state);
            let _ = left.present();
            let _ = right.present();
            let _ = top.present();
            left.set_visible(false);
            right.set_visible(false);
            top.set_visible(false);
            was_visible = false;
        }

        if !is_enabled {
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        // Re-assert TOPMOST every ~2 seconds (120 frames at 16ms)
        topmost_counter += 1;
        if topmost_counter % 120 == 0 {
            left.force_topmost();
            right.force_topmost();
            top.force_topmost();
        }

        // Read latest snapshot
        let new_raw = if let Ok(s) = snapshot.read() {
            if s.capture_active {
                Some(OverlayState {
                    far_left: s.far_left,
                    left: s.left,
                    center: s.center,
                    right: s.right,
                    far_right: s.far_right,
                    ambience: s.ambience,
                })
            } else {
                None
            }
        } else {
            None
        };

        if let Some(raw) = new_raw {
            state = smooth_overlay(&raw, &state, 0.55, 0.18);
        } else {
            state = smooth_overlay(&OverlayState::default(), &state, 0.55, 0.06);
        }

        // Log periodically to confirm overlay is rendering
        log_counter += 1;
        if log_counter % 300 == 1 {
            tracing::debug!(
                "Overlay render #{}: fl={:.3} l={:.3} c={:.3} r={:.3} fr={:.3} amb={:.3}",
                log_counter,
                state.far_left,
                state.left,
                state.center,
                state.right,
                state.far_right,
                state.ambience
            );
        }

        left.render(&state);
        right.render(&state);
        top.render(&state);

        // Present is non-fatal — a transient GDI failure shouldn't kill the overlay
        if let Err(e) = left.present() {
            present_errors += 1;
            if present_errors <= 3 {
                overlay_log(&format!("present(left) failed: {:#}", e));
            }
        }
        if let Err(e) = right.present() {
            present_errors += 1;
            if present_errors <= 3 {
                overlay_log(&format!("present(right) failed: {:#}", e));
            }
        }
        if let Err(e) = top.present() {
            present_errors += 1;
            if present_errors <= 3 {
                overlay_log(&format!("present(top) failed: {:#}", e));
            }
        }

        thread::sleep(FRAME_TIME);
    }
}

fn smooth_overlay(c: &OverlayState, p: &OverlayState, atk: f32, rel: f32) -> OverlayState {
    OverlayState {
        far_left: blend_ar(p.far_left, c.far_left, atk, rel),
        left: blend_ar(p.left, c.left, atk, rel),
        center: blend_ar(p.center, c.center, atk, rel),
        right: blend_ar(p.right, c.right, atk, rel),
        far_right: blend_ar(p.far_right, c.far_right, atk, rel),
        ambience: blend_ar(p.ambience, c.ambience, atk, rel),
    }
}

fn blend_ar(prev: f32, cur: f32, atk: f32, rel: f32) -> f32 {
    let a = if cur > prev { atk } else { rel };
    a * cur + (1.0 - a) * prev
}

// ── Win32 window infrastructure ──

fn register_window_class() -> Result<()> {
    let instance = unsafe { GetModuleHandleW(None) }?;
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default();
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        hCursor: cursor,
        hbrBackground: HBRUSH(null_mut()),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    let atom = unsafe { RegisterClassExW(&class) };
    if atom == 0 {
        return Err(anyhow!("RegisterClassExW failed"));
    }
    Ok(())
}

fn process_messages() -> bool {
    let mut msg = MSG::default();
    loop {
        let has = unsafe { PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE) }.as_bool();
        if !has {
            return false;
        }
        if msg.message == WM_QUIT {
            return true;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_CLOSE | WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ── LayeredWindow ──

#[derive(Clone, Copy)]
enum WindowRole {
    Left,
    Right,
    Top,
}

struct LayeredWindow {
    hwnd: HWND,
    role: WindowRole,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pixels: Vec<u8>,
    // Pre-allocated GDI resources — reused every frame
    mem_dc: windows::Win32::Graphics::Gdi::HDC,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    bits_ptr: *mut u8,
    previous_obj: HGDIOBJ,
}

impl LayeredWindow {
    fn new(role: WindowRole, x: i32, y: i32, w: i32, h: i32) -> Result<Self> {
        let instance = unsafe { GetModuleHandleW(None) }?;
        let ex =
            WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let hwnd = unsafe {
            CreateWindowExW(
                ex,
                CLASS_NAME,
                w!("VanySound Radar"),
                WS_POPUP,
                x,
                y,
                w,
                h,
                None,
                None,
                instance,
                None,
            )
        }?;
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }

        // Pre-allocate GDI resources once
        let screen_dc = unsafe { GetDC(HWND::default()) };
        if screen_dc.0.is_null() {
            return Err(anyhow!("GetDC failed during init"));
        }
        let mem_dc = unsafe { CreateCompatibleDC(screen_dc) };
        unsafe {
            ReleaseDC(HWND::default(), screen_dc);
        }
        if mem_dc.0.is_null() {
            return Err(anyhow!("CreateCompatibleDC failed during init"));
        }

        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };

        let mut bits: *mut c_void = null_mut();
        let bitmap = unsafe { CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) }?;
        let previous_obj = unsafe { SelectObject(mem_dc, HGDIOBJ(bitmap.0)) };

        Ok(Self {
            hwnd,
            role,
            x,
            y,
            width: w,
            height: h,
            pixels: vec![0; (w * h * 4) as usize],
            mem_dc,
            bitmap,
            bits_ptr: bits.cast::<u8>(),
            previous_obj,
        })
    }

    fn set_visible(&self, visible: bool) {
        unsafe {
            if visible {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            } else {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    fn force_topmost(&self) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                self.x,
                self.y,
                self.width,
                self.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    fn render(&mut self, r: &OverlayState) {
        self.pixels.fill(0);
        let s = OverlayState {
            far_left: soft_ceil(r.far_left),
            left: soft_ceil(r.left),
            center: soft_ceil(r.center),
            right: soft_ceil(r.right),
            far_right: soft_ceil(r.far_right),
            ambience: soft_ceil(r.ambience),
        };
        match self.role {
            WindowRole::Left => self.render_side(s.far_left, s.left, s.ambience, true),
            WindowRole::Right => self.render_side(s.far_right, s.right, s.ambience, false),
            WindowRole::Top => self.render_top(&s),
        }
    }

    fn render_side(&mut self, outer: f32, inner: f32, amb: f32, is_left: bool) {
        let (w, h) = (self.width as usize, self.height as usize);
        let base = (inner * 0.95 + outer * 0.80 + amb * 0.50).clamp(0.0, 1.0);
        if base <= 0.001 {
            return;
        }
        let strength = inner.max(outer);
        let tighten = 1.0 + (strength - SOFT_KNEE).max(0.0) * 2.8;

        for y in 0..h {
            let y_norm = y as f32 / h as f32;
            let vert = 0.45 + (1.0 - (y_norm - 0.48).abs() * 1.2).max(0.0) * 0.55;
            for x in 0..w {
                let et = if is_left {
                    1.0 - x as f32 / w as f32
                } else {
                    x as f32 / w as f32
                };
                let ig = et.powf(2.10 * tighten) * vert * inner;
                let og = et.powf(1.35 * tighten) * (0.25 + (1.0 - et) * 0.75) * vert * outer;
                let glow = (ig + og + et.powf(2.05 * tighten) * amb * 0.35).clamp(0.0, 1.0);
                if glow <= 0.002 {
                    continue;
                }
                let idx = ((y * w) + x) * 4;
                let color = if is_left {
                    [0x18, 0xE6, 0xFF]
                } else {
                    [0x1C, 0xAE, 0xFF]
                };
                add_premul(&mut self.pixels, idx, color, glow * 0.92);
                let tracer = et.powf(6.0) * inner.max(outer * 0.55).max(amb * 0.4) * 0.55;
                if tracer > 0.003 {
                    add_premul(&mut self.pixels, idx, [0x9A, 0xFF, 0xFF], tracer);
                }
            }
        }
    }

    fn render_top(&mut self, r: &OverlayState) {
        let (w, h) = (self.width as usize, self.height as usize);
        let sy = h as f32 * 0.34;
        let rx = self.width as f32 * 0.11;
        let ry = self.height as f32 * 0.85;

        if r.far_left <= 0.001
            && r.left <= 0.001
            && r.center <= 0.001
            && r.right <= 0.001
            && r.far_right <= 0.001
        {
            return;
        }

        // 7 hotspot positions for 7.1 layout
        let positions = [
            (0.05f32, [0x0E, 0x99, 0xFF], r.far_left * 0.88), // Back Left
            (0.18, [0x12, 0xBF, 0xFF], r.far_left * 0.94),    // Side Left
            (0.32, [0x14, 0xE6, 0xFF], r.left * 0.92),        // Front Left
            (0.50, [0x8E, 0xFF, 0xFF], r.center),             // Center
            (0.68, [0x1C, 0xB2, 0xFF], r.right * 0.92),       // Front Right
            (0.82, [0x1A, 0x8B, 0xFF], r.far_right * 0.94),   // Side Right
            (0.95, [0x10, 0x70, 0xFF], r.far_right * 0.88),   // Back Right
        ];

        for &(x_pct, color, intensity) in &positions {
            self.draw_hotspot(
                self.width as f32 * x_pct,
                sy,
                rx * 0.92,
                ry,
                color,
                intensity,
            );
        }

        // Ambient wash
        for y in 0..h {
            let yn = y as f32 / h as f32;
            let v = (1.0 - yn).powf(1.8) * r.ambience * 0.65;
            if v <= 0.002 {
                continue;
            }
            for x in 0..w {
                let xn = x as f32 / w as f32;
                let cp = 1.0 - ((xn - 0.5).abs() * 1.55).min(1.0);
                let a = v * (0.28 + cp * 0.72);
                if a <= 0.002 {
                    continue;
                }
                let idx = ((y * w) + x) * 4;
                add_premul(&mut self.pixels, idx, [0x2B, 0x99, 0xFF], a * 0.48);
            }
        }
    }

    fn draw_hotspot(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: [u8; 3], intensity: f32) {
        if intensity <= 0.001 {
            return;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        for y in 0..h {
            let dy = (y as f32 - cy) / ry;
            let dy2 = dy * dy;
            if dy2 >= 1.0 {
                continue;
            }
            for x in 0..w {
                let dx = (x as f32 - cx) / rx;
                let d = dx * dx + dy2;
                if d >= 1.0 {
                    continue;
                }
                let bloom = (1.0 - d).powf(2.4 + intensity * 1.8) * intensity;
                if bloom <= 0.002 {
                    continue;
                }
                add_premul(&mut self.pixels, ((y * w) + x) * 4, color, bloom * 0.92);
            }
        }
    }

    fn present(&self) -> Result<()> {
        // Copy pixel data to the pre-allocated DIB bits
        unsafe {
            std::ptr::copy_nonoverlapping(self.pixels.as_ptr(), self.bits_ptr, self.pixels.len());
        }

        let screen_dc = unsafe { GetDC(HWND::default()) };
        if screen_dc.0.is_null() {
            return Err(anyhow!("GetDC failed"));
        }

        let dst = POINT {
            x: self.x,
            y: self.y,
        };
        let src = POINT { x: 0, y: 0 };
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        let result = unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                screen_dc,
                Some(&dst),
                Some(&size),
                self.mem_dc,
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        };

        unsafe {
            ReleaseDC(HWND::default(), screen_dc);
        }
        result.ok().context("UpdateLayeredWindow failed")
    }
}

impl Drop for LayeredWindow {
    fn drop(&mut self) {
        unsafe {
            // Restore original GDI object before deleting
            if !self.mem_dc.0.is_null() {
                SelectObject(self.mem_dc, self.previous_obj);
            }
            if !self.bitmap.0.is_null() {
                let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            }
            if !self.mem_dc.0.is_null() {
                let _ = DeleteDC(self.mem_dc);
            }
            if !self.hwnd.0.is_null() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

fn soft_ceil(x: f32) -> f32 {
    if x <= SOFT_KNEE {
        return x;
    }
    let t = (x - SOFT_KNEE) / (1.0 - SOFT_KNEE);
    SOFT_KNEE + (SOFT_MAX - SOFT_KNEE) * t.powf(0.45)
}

fn add_premul(buf: &mut [u8], idx: usize, color: [u8; 3], alpha: f32) {
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u16;
    if a == 0 {
        return;
    }
    buf[idx] = buf[idx].saturating_add((color[2] as u16 * a / 255) as u8); // B
    buf[idx + 1] = buf[idx + 1].saturating_add((color[1] as u16 * a / 255) as u8); // G
    buf[idx + 2] = buf[idx + 2].saturating_add((color[0] as u16 * a / 255) as u8); // R
    buf[idx + 3] = buf[idx + 3].saturating_add(a as u8); // A
}
