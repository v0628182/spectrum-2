use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use crossbeam_channel::Receiver;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
    ReleaseDC, SelectObject, HBRUSH, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetSystemMetrics,
    LoadCursorW, PeekMessageW, PostQuitMessage, RegisterClassExW, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, HTTRANSPARENT, IDC_ARROW, MSG, PM_REMOVE,
    SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNOACTIVATE, ULW_ALPHA, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_NCHITTEST, WM_QUIT, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::analysis::{self, RadarState};

const EDGE_WIDTH: i32 = 112;
const TOP_HEIGHT: i32 = 148;
const FRAME_TIME: Duration = Duration::from_millis(16);
/// Sonidos por debajo del knee se renderizan sin cambio (pasos, susurros).
/// Por encima, la intensidad se comprime hacia SOFT_MAX.
const SOFT_KNEE: f32 = 0.28;
const SOFT_MAX: f32 = 0.55;
const CLASS_NAME: PCWSTR = w!("EchoAudioRadarOverlay");

pub fn run(rx: Receiver<crate::audio::AudioPacket>) -> Result<()> {
    register_window_class().context("no se pudo registrar la clase Win32 del overlay")?;

    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    if screen_w <= 0 || screen_h <= 0 {
        return Err(anyhow!("no se pudo obtener el tamano de pantalla"));
    }

    tracing::info!("Overlay target screen: {}x{}", screen_w, screen_h);

    let top_width = (screen_w - EDGE_WIDTH * 2).max(320);

    let mut left = LayeredWindow::new(WindowRole::Left, 0, 0, EDGE_WIDTH, screen_h)?;
    let mut right = LayeredWindow::new(
        WindowRole::Right,
        screen_w - EDGE_WIDTH,
        0,
        EDGE_WIDTH,
        screen_h,
    )?;
    let mut top = LayeredWindow::new(WindowRole::Top, EDGE_WIDTH, 0, top_width, TOP_HEIGHT)?;

    left.present()?;
    right.present()?;
    top.present()?;

    let mut state = RadarState::default();

    loop {
        if process_messages() {
            return Ok(());
        }

        let mut latest_state: Option<RadarState> = None;
        while let Ok((samples, channels, channel_mask)) = rx.try_recv() {
            latest_state = Some(analysis::analyze_radar(&samples, channels, channel_mask));
        }

        if let Some(new_state) = latest_state {
            state = analysis::smooth_radar(&new_state, &state, 0.55, 0.18);
        } else {
            state = analysis::smooth_radar(&RadarState::default(), &state, 0.55, 0.06);
        }

        left.render(&state);
        right.render(&state);
        top.render(&state);

        left.present()?;
        right.present()?;
        top.present()?;

        thread::sleep(FRAME_TIME);
    }
}

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
        return Err(anyhow!("RegisterClassExW fallo para la clase del overlay"));
    }

    Ok(())
}

fn process_messages() -> bool {
    let mut msg = MSG::default();
    loop {
        let has_message =
            unsafe { PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE) }.as_bool();
        if !has_message {
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

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_CLOSE => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

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
}

impl LayeredWindow {
    fn new(role: WindowRole, x: i32, y: i32, width: i32, height: i32) -> Result<Self> {
        let instance = unsafe { GetModuleHandleW(None) }?;
        let ex_style: WINDOW_EX_STYLE = WS_EX_TOPMOST
            | WS_EX_TRANSPARENT
            | WS_EX_LAYERED
            | WS_EX_TOOLWINDOW
            | WS_EX_NOACTIVATE;
        let style: WINDOW_STYLE = WS_POPUP;

        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                CLASS_NAME,
                w!("EchoAudio Radar"),
                style,
                x,
                y,
                width,
                height,
                None,
                None,
                instance,
                None,
            )
        }?;

        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }

        Ok(Self {
            hwnd,
            role,
            x,
            y,
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        })
    }

    fn render(&mut self, radar: &RadarState) {
        self.pixels.fill(0);

        // Techo visual: pasos intactos, sonidos fuertes comprimidos
        let r = RadarState {
            far_left: soft_ceil(radar.far_left),
            left: soft_ceil(radar.left),
            center: soft_ceil(radar.center),
            right: soft_ceil(radar.right),
            far_right: soft_ceil(radar.far_right),
            ambience: soft_ceil(radar.ambience),
        };

        match self.role {
            WindowRole::Left => self.render_side(r.far_left, r.left, r.ambience, true),
            WindowRole::Right => self.render_side(r.far_right, r.right, r.ambience, false),
            WindowRole::Top => self.render_top_stage(&r),
        }
    }

    fn render_side(&mut self, outer: f32, inner: f32, ambience: f32, is_left: bool) {
        let width = self.width as usize;
        let height = self.height as usize;
        let base_strength = (inner * 0.95 + outer * 0.80 + ambience * 0.50).clamp(0.0, 1.0);
        if base_strength <= 0.001 {
            return;
        }

        // Sonidos fuertes → glow más concentrado cerca del borde (menos espacio)
        let strength = inner.max(outer);
        let tighten = 1.0 + (strength - SOFT_KNEE).max(0.0) * 2.8;

        for y in 0..height {
            let y_norm = y as f32 / height as f32;
            let vertical_body = 0.45 + (1.0 - (y_norm - 0.48).abs() * 1.2).max(0.0) * 0.55;

            for x in 0..width {
                let edge_t = if is_left {
                    1.0 - x as f32 / width as f32
                } else {
                    x as f32 / width as f32
                };
                let inner_glow = edge_t.powf(2.10 * tighten) * vertical_body * inner;
                let outer_glow =
                    edge_t.powf(1.35 * tighten) * (0.25 + (1.0 - edge_t) * 0.75) * vertical_body * outer;
                let glow = (inner_glow + outer_glow + edge_t.powf(2.05 * tighten) * ambience * 0.35)
                    .clamp(0.0, 1.0);
                if glow <= 0.002 {
                    continue;
                }

                let idx = ((y * width) + x) * 4;
                let alpha = glow * 0.92;
                let color = if is_left {
                    [0x18, 0xE6, 0xFF]
                } else {
                    [0x1C, 0xAE, 0xFF]
                };
                add_premultiplied(&mut self.pixels, idx, color, alpha);

                let tracer = edge_t.powf(6.0) * inner.max(outer * 0.55).max(ambience * 0.4) * 0.55;
                if tracer > 0.003 {
                    add_premultiplied(&mut self.pixels, idx, [0x9A, 0xFF, 0xFF], tracer);
                }
            }
        }
    }

    fn render_top_stage(&mut self, radar: &RadarState) {
        let width = self.width as usize;
        let height = self.height as usize;
        let stage_y = height as f32 * 0.34;
        let radius_x = self.width as f32 * 0.11;
        let radius_y = self.height as f32 * 0.85;

        if radar.far_left <= 0.001
            && radar.left <= 0.001
            && radar.center <= 0.001
            && radar.right <= 0.001
            && radar.far_right <= 0.001
        {
            return;
        }

        let far_left_x = self.width as f32 * 0.08;
        let left_x = self.width as f32 * 0.26;
        let center_x = self.width as f32 * 0.50;
        let right_x = self.width as f32 * 0.74;
        let far_right_x = self.width as f32 * 0.92;

        self.draw_hotspot(
            far_left_x,
            stage_y,
            radius_x * 0.92,
            radius_y,
            [0x12, 0xBF, 0xFF],
            radar.far_left * 0.94,
        );
        self.draw_hotspot(
            left_x,
            stage_y,
            radius_x,
            radius_y,
            [0x14, 0xE6, 0xFF],
            radar.left * 0.92,
        );
        self.draw_hotspot(
            center_x,
            stage_y,
            radius_x * 0.95,
            radius_y,
            [0x8E, 0xFF, 0xFF],
            radar.center,
        );
        self.draw_hotspot(
            right_x,
            stage_y,
            radius_x,
            radius_y,
            [0x1C, 0xB2, 0xFF],
            radar.right * 0.92,
        );
        self.draw_hotspot(
            far_right_x,
            stage_y,
            radius_x * 0.92,
            radius_y,
            [0x1A, 0x8B, 0xFF],
            radar.far_right * 0.94,
        );

        for y in 0..height {
            let y_norm = y as f32 / height as f32;
            let vertical = (1.0 - y_norm).powf(1.8) * radar.ambience * 0.65;
            if vertical <= 0.002 {
                continue;
            }

            for x in 0..width {
                let x_norm = x as f32 / width as f32;
                let center_pull = 1.0 - ((x_norm - 0.5).abs() * 1.55).min(1.0);
                let alpha = vertical * (0.28 + center_pull * 0.72);
                if alpha <= 0.002 {
                    continue;
                }

                let idx = ((y * width) + x) * 4;
                add_premultiplied(&mut self.pixels, idx, [0x2B, 0x99, 0xFF], alpha * 0.48);
            }
        }
    }

    fn draw_hotspot(
        &mut self,
        center_x: f32,
        center_y: f32,
        radius_x: f32,
        radius_y: f32,
        color: [u8; 3],
        intensity: f32,
    ) {
        if intensity <= 0.001 {
            return;
        }

        let width = self.width as usize;
        let height = self.height as usize;

        for y in 0..height {
            let dy = (y as f32 - center_y) / radius_y;
            let dy2 = dy * dy;
            if dy2 >= 1.0 {
                continue;
            }

            for x in 0..width {
                let dx = (x as f32 - center_x) / radius_x;
                let distance = dx * dx + dy2;
                if distance >= 1.0 {
                    continue;
                }

                let bloom = (1.0 - distance).powf(2.4 + intensity * 1.8) * intensity;
                if bloom <= 0.002 {
                    continue;
                }

                let idx = ((y * width) + x) * 4;
                add_premultiplied(&mut self.pixels, idx, color, bloom * 0.92);
            }
        }
    }

    fn present(&self) -> Result<()> {
        let screen_dc = unsafe { GetDC(HWND::default()) };
        if screen_dc.0.is_null() {
            return Err(anyhow!("GetDC devolvio un DC invalido"));
        }

        let mem_dc = unsafe { CreateCompatibleDC(screen_dc) };
        if mem_dc.0.is_null() {
            unsafe {
                ReleaseDC(HWND::default(), screen_dc);
            }
            return Err(anyhow!("CreateCompatibleDC fallo"));
        }

        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: self.width,
            biHeight: -self.height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };

        let mut bits: *mut c_void = std::ptr::null_mut();
        let bitmap = unsafe {
            CreateDIBSection(
                mem_dc,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )
        }?;

        unsafe {
            std::ptr::copy_nonoverlapping(
                self.pixels.as_ptr(),
                bits.cast::<u8>(),
                self.pixels.len(),
            );
        }

        let previous = unsafe { SelectObject(mem_dc, HGDIOBJ(bitmap.0)) };
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
                mem_dc,
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        };

        unsafe {
            SelectObject(mem_dc, previous);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(mem_dc);
            ReleaseDC(HWND::default(), screen_dc);
        }

        result.ok().context("UpdateLayeredWindow fallo")
    }
}

impl Drop for LayeredWindow {
    fn drop(&mut self) {
        if !self.hwnd.0.is_null() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

/// Techo visual suave: valores <= SOFT_KNEE pasan intactos (pasos),
/// valores superiores se comprimen hacia SOFT_MAX (sonidos fuertes más sutiles).
fn soft_ceil(x: f32) -> f32 {
    if x <= SOFT_KNEE {
        return x;
    }
    let t = (x - SOFT_KNEE) / (1.0 - SOFT_KNEE);
    SOFT_KNEE + (SOFT_MAX - SOFT_KNEE) * t.powf(0.45)
}

fn add_premultiplied(buffer: &mut [u8], idx: usize, color: [u8; 3], alpha: f32) {
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u16;
    if a == 0 {
        return;
    }

    let r = (color[0] as u16 * a / 255) as u8;
    let g = (color[1] as u16 * a / 255) as u8;
    let b = (color[2] as u16 * a / 255) as u8;

    buffer[idx] = buffer[idx].saturating_add(b);
    buffer[idx + 1] = buffer[idx + 1].saturating_add(g);
    buffer[idx + 2] = buffer[idx + 2].saturating_add(r);
    buffer[idx + 3] = buffer[idx + 3].saturating_add(a as u8);
}
