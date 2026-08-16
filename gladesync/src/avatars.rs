/// Player avatar rendering: transparent ball + arrow + pseudo on a screen overlay.
///
/// Creates a full-screen transparent overlay window that draws other players'
/// positions projected from 3D world space to 2D screen coordinates.
use crate::hook::HookEngine;
use crate::network::NetworkManager;
use std::sync::Arc;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, BOOL, TRUE, FALSE};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// Magenta as transparency color key (0x00BBGGRR format)
const COLOR_KEY: u32 = 0x00FF00FF;

// Window class name
const CLASS_NAME: &[u16] = &[
    b'G' as u16, b'l' as u16, b'a' as u16, b'd' as u16, b'e' as u16,
    b'S' as u16, b'y' as u16, b'n' as u16, b'c' as u16, b'A' as u16,
    b'v' as u16, 0,
];

/// Global state accessible from the window procedure.
struct AvatarState {
    network: Arc<NetworkManager>,
}

static mut AVATAR_STATE: Option<AvatarState> = None;

/// Start the avatar overlay in a dedicated thread.
pub fn start_avatars_thread(network: Arc<NetworkManager>) {
    thread::spawn(move || {
        unsafe {
            AVATAR_STATE = Some(AvatarState {
                network: network,
            });
        }
        run_avatar_window();
    });
}

unsafe extern "system" fn avatar_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            on_paint(hwnd);
            0
        }
        WM_TIMER => {
            InvalidateRect(hwnd, std::ptr::null(), FALSE);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn on_paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);

    if hdc == 0 {
        return;
    }

    // Get screen dimensions
    let screen_w = GetSystemMetrics(SM_CXSCREEN) as i32;
    let screen_h = GetSystemMetrics(SM_CYSCREEN) as i32;

    // Fill background with color key (transparent)
    let bg_brush = CreateSolidBrush(COLOR_KEY);
    let mut rc = RECT {
        left: 0, top: 0, right: screen_w, bottom: screen_h,
    };
    FillRect(hdc, &rc, bg_brush);
    DeleteObject(bg_brush as _);

    // Get camera data
    let camera = match HookEngine::read_camera_data() {
        Some(c) => c,
        None => {
            // Camera not found yet — show waiting message
            draw_text_centered(hdc, screen_w / 2, screen_h / 2,
                "Searching for camera... Move around!");
            EndPaint(hwnd, &ps);
            return;
        }
    };

    let (cam_x, cam_y, cam_z, cam_yaw, cam_pitch) = camera;

    // Get remote player transforms
    let state = &*AVATAR_STATE.as_ref().unwrap();
    let transforms = state.network.get_remote_transforms();

    if transforms.is_empty() {
        EndPaint(hwnd, &ps);
        return;
    }

    // Draw each player
    for t in &transforms {
        // Project 3D world position to 2D screen coordinates
        if let Some((sx, sy, depth)) = project_to_screen(
            t.x, t.y, t.z,
            cam_x, cam_y, cam_z,
            cam_yaw, cam_pitch,
            screen_w as f32, screen_h as f32,
        ) {
            if depth > 0.0 {
                draw_avatar(hdc, sx as i32, sy as i32, &t.pseudo, t.yaw, t.pitch, cam_yaw, cam_pitch, depth, screen_w, screen_h);
            }
        }
    }

    EndPaint(hwnd, &ps);
}

/// Project a 3D world position to 2D screen coordinates.
/// Returns Some((screen_x, screen_y, depth)) where depth > 0 means in front of camera.
fn project_to_screen(
    px: f32, py: f32, pz: f32,
    cx: f32, cy: f32, cz: f32,
    yaw: f32, pitch: f32,
    screen_w: f32, screen_h: f32,
) -> Option<(f32, f32, f32)> {
    // Relative position
    let rel_x = px - cx;
    let rel_y = py - cy;
    let rel_z = pz - cz;

    // Camera forward vector (yaw rotates around Y, pitch around X)
    let cos_y = yaw.cos();
    let sin_y = yaw.sin();
    let cos_p = pitch.cos();
    let sin_p = pitch.sin();

    // Apply yaw rotation (around Y axis)
    let vx = rel_x * cos_y + rel_z * sin_y;
    let vz = -rel_x * sin_y + rel_z * cos_y;
    let vy = rel_y;

    // Apply pitch rotation (around X axis)
    let vy2 = vy * cos_p - vz * sin_p;
    let vz2 = vy * sin_p + vz * cos_p;
    let vx2 = vx;

    // Perspective projection (only if in front of camera)
    if vz2 > 0.1 {
        let fov = 60.0f32.to_radians();
        let f = 1.0 / (fov / 2.0).tan();
        let half_w = screen_w / 2.0;
        let half_h = screen_h / 2.0;
        let sx = (vx2 * f / vz2) * half_w + half_w;
        let sy = -(vy2 * f / vz2) * half_h + half_h;
        Some((sx, sy, vz2))
    } else {
        None
    }
}

/// Draw a single player avatar: transparent ball + arrow + pseudo.
unsafe fn draw_avatar(
    hdc: HDC,
    sx: i32, sy: i32,
    pseudo: &str,
    _player_yaw: f32, _player_pitch: f32,
    _cam_yaw: f32, _cam_pitch: f32,
    _depth: f32,
    _screen_w: i32, _screen_h: i32,
) {
    // Ball size (could scale with distance, but keep simple for now)
    let radius = 20i32;

    // Draw ball outline (cyan circle)
    let pen = CreatePen(PS_SOLID, 2, 0x00FFFF00); // Cyan
    let old_pen = SelectObject(hdc, pen as _);
    let brush = GetStockObject(NULL_BRUSH);
    let old_brush = SelectObject(hdc, brush);

    Ellipse(hdc, sx - radius, sy - radius, sx + radius, sy + radius);

    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(pen as _);

    // Draw arrow (direction the player is looking)
    // For now, just draw a small line pointing up from the ball
    let arrow_len = radius + 10;
    let arrow_pen = CreatePen(PS_SOLID, 3, 0x0000FFFF); // Yellow (BGR)
    let old_pen2 = SelectObject(hdc, arrow_pen as _);
    MoveToEx(hdc, sx, sy - radius, std::ptr::null_mut());
    LineTo(hdc, sx, sy - radius - arrow_len);
    // Arrowhead
    LineTo(hdc, sx - 5, sy - radius - arrow_len + 8);
    MoveToEx(hdc, sx, sy - radius - arrow_len, std::ptr::null_mut());
    LineTo(hdc, sx + 5, sy - radius - arrow_len + 8);
    SelectObject(hdc, old_pen2);
    DeleteObject(arrow_pen as _);

    // Draw pseudo text above the ball
    let text: Vec<u16> = pseudo.encode_utf16().collect();
    let text_y = sy - radius - arrow_len - 20;

    // Text background (dark)
    let bg_rect = RECT {
        left: sx - 60,
        top: text_y - 2,
        right: sx + 60,
        bottom: text_y + 18,
    };
    let text_bg = CreateSolidBrush(0x00302010); // Dark brown
    FillRect(hdc, &bg_rect, text_bg);
    DeleteObject(text_bg as _);

    // Text (white)
    SetTextColor(hdc, 0x00FFFFFF);
    SetBkMode(hdc, TRANSPARENT);

    let mut text_buf: [u16; 64] = [0; 64];
    let len = text.len().min(63);
    text_buf[..len].copy_from_slice(&text[..len]);

    // Center the text
    let mut text_rect = bg_rect;
    DrawTextW(
        hdc,
        text_buf.as_ptr(),
        len as i32,
        &mut text_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}

/// Draw centered text (for status messages).
unsafe fn draw_text_centered(hdc: HDC, x: i32, y: i32, text: &str) {
    let text_utf16: Vec<u16> = text.encode_utf16().collect();
    let mut buf: [u16; 128] = [0; 128];
    let len = text_utf16.len().min(127);
    buf[..len].copy_from_slice(&text_utf16[..len]);

    let mut rect = RECT {
        left: x - 150,
        top: y - 15,
        right: x + 150,
        bottom: y + 15,
    };

    // Background
    let bg = CreateSolidBrush(0x00302010);
    FillRect(hdc, &rect, bg);
    DeleteObject(bg as _);

    SetTextColor(hdc, 0x00FFFFFF);
    SetBkMode(hdc, TRANSPARENT);

    DrawTextW(hdc, buf.as_ptr(), len as i32, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
}

/// Create and run the avatar overlay window.
fn run_avatar_window() {
    unsafe {
        let hinst = 0usize;

        // Register window class
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(avatar_wndproc),
            hInstance: hinst,
            lpszClassName: CLASS_NAME.as_ptr(),
            hCursor: LoadCursorW(0, IDC_ARROW),
            hbrBackground: CreateSolidBrush(COLOR_KEY) as _,
            ..Default::default()
        };

        if RegisterClassW(&wc) == 0 {
            eprintln!("[GladeSync Avatars] Failed to register window class");
            return;
        }

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST,
            CLASS_NAME.as_ptr(),
            [b'A' as u16, b'v' as u16, b'a' as u16, b't' as u16, b'a' as u16, b'r' as u16, b's' as u16, 0].as_ptr(),
            WS_POPUP,
            0, 0, screen_w, screen_h,
            0, 0, hinst,
            std::ptr::null_mut(),
        );

        if hwnd == 0 {
            eprintln!("[GladeSync Avatars] Failed to create overlay window");
            return;
        }

        // Set color key for transparency (magenta = transparent)
        SetLayeredWindowAttributes(hwnd, COLOR_KEY, 255, LWA_COLORKEY);

        ShowWindow(hwnd, SW_SHOWNORMAL);
        UpdateWindow(hwnd);

        // Set a timer for ~30 FPS repaint
        SetTimer(hwnd, 1, 33, None);

        println!("\x1b[32m[GladeSync Avatars] Overlay window active\x1b[0m");

        // Message loop
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
