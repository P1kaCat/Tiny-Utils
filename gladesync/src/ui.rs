use crate::network::NetworkManager;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreatePen,
    CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    FillRect, GetDC, InvalidateRect, ReleaseDC, RoundRect, SelectObject, SetBkColor,
    SetBkMode, SetTextColor, SetWindowRgn, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, DT_CENTER, DT_SINGLELINE, DT_VCENTER, PAINTSTRUCT, PS_SOLID,
    TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetClientRect,
    GetForegroundWindow, GetMessageW, GetWindowLongPtrW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, LoadCursorW, MoveWindow,
    PostQuitMessage, RegisterClassExW, SetCursor, SetTimer, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW,
    ES_AUTOHSCROLL, GWL_EXSTYLE, GWLP_USERDATA, IDC_ARROW, MSG, SW_HIDE, SW_SHOW,
    SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, ULW_ALPHA, WM_CREATE,
    WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_PAINT, WM_SETCURSOR, WM_TIMER,
    WNDCLASSEXW, WS_BORDER, WS_CHILD, WS_CLIPCHILDREN, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

static IS_MENU_OPEN: AtomicBool = AtomicBool::new(false);

const ID_EDIT_PSEUDO: isize = 1000;
const ID_EDIT_IP: isize = 1001;
const ID_EDIT_PORT: isize = 1002;
const TIMER_REFRESH: usize = 1;

const OFFSET_X: i32 = 20;
const OFFSET_Y: i32 = 20;
const OPEN_W: i32 = 420;
const OPEN_H: i32 = 470;
const CLOSED_W: i32 = 56;
const CLOSED_H: i32 = 56;

/// WM_CTLCOLOREDIT = 0x0133
const WM_CTLCOLOREDIT: u32 = 0x0133;

/// Background color: Ivory parchment F8F4EB → COLORREF (BGR) = 0x00EBF4F8
const BG_COLOR: u32 = 0x00EBF4F8;

/// Player list area Y range
const PLAYER_LIST_TOP: i32 = 290;
const PLAYER_LIST_BOTTOM: i32 = 410;
const PLAYER_ROW_H: i32 = 28;

struct UIState {
    network: Arc<NetworkManager>,
    game_hwnd: HWND,
    hwnd_pseudo: HWND,
    hwnd_ip: HWND,
    hwnd_port: HWND,
    status_text: String,
    last_game_pos: (i32, i32),
    edit_bg_brush: *mut c_void,
    last_player_count: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// Game window finder
// ─────────────────────────────────────────────────────────────────────────────

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == GetCurrentProcessId() && IsWindowVisible(hwnd) != 0 {
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        if (rect.right - rect.left) > 200 && (rect.bottom - rect.top) > 200 {
            let out_ptr = lparam as *mut HWND;
            *out_ptr = hwnd;
            return 0;
        }
    }
    1
}

fn find_game_window() -> HWND {
    let mut target_hwnd: HWND = std::ptr::null_mut();
    for _ in 0..50 {
        unsafe {
            EnumWindows(Some(enum_proc), &mut target_hwnd as *mut HWND as isize);
        }
        if !target_hwnd.is_null() {
            return target_hwnd;
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }
    std::ptr::null_mut()
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn start_ui_thread(network: Arc<NetworkManager>) {
    thread::spawn(move || unsafe {
        let game_hwnd = find_game_window();

        let instance = GetModuleHandleW(std::ptr::null());
        let class_name: Vec<u16> = "TinyUtilsUI\0".encode_utf16().collect();
        let arrow_cursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: arrow_cursor,
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wnd_class);

        let window_title: Vec<u16> = "Tiny Utils\0".encode_utf16().collect();

        let mut game_rect: RECT = std::mem::zeroed();
        if !game_hwnd.is_null() {
            GetWindowRect(game_hwnd, &mut game_rect);
        }

        let state = Box::into_raw(Box::new(UIState {
            network: Arc::clone(&network),
            game_hwnd,
            hwnd_pseudo: std::ptr::null_mut(),
            hwnd_ip: std::ptr::null_mut(),
            hwnd_port: std::ptr::null_mut(),
            status_text: "Ready to play".to_string(),
            last_game_pos: (game_rect.left, game_rect.top),
            edit_bg_brush: CreateSolidBrush(0x00FFFFFF) as *mut c_void,
            last_player_count: 0,
        }));

        // Start as a layered popup (for the translucent star button)
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_POPUP | WS_VISIBLE | WS_CLIPCHILDREN,
            game_rect.left + OFFSET_X,
            game_rect.top + OFFSET_Y,
            CLOSED_W,
            CLOSED_H,
            game_hwnd,
            std::ptr::null_mut(),
            instance,
            state as *mut c_void,
        );

        if hwnd.is_null() {
            return;
        }

        // Render the initial star button
        render_star_button(hwnd, &*state);
        SetTimer(hwnd, TIMER_REFRESH, 30, None);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Star button renderer (layered mode only, via tiny-skia + UpdateLayeredWindow)
// ─────────────────────────────────────────────────────────────────────────────

unsafe fn render_star_button(hwnd: HWND, state: &UIState) {
    let w = CLOSED_W as u32;
    let h = CLOSED_H as u32;

    let mut pixmap = match Pixmap::new(w, h) {
        Some(p) => p,
        None => return,
    };

    let w_f = w as f32;
    let h_f = h as f32;

    // Frosted glass rounded rectangle
    let r = 16.0f32;
    let mut pb = PathBuilder::new();
    pb.move_to(1.5 + r, 1.5);
    pb.line_to(w_f - 1.5 - r, 1.5);
    pb.quad_to(w_f - 1.5, 1.5, w_f - 1.5, 1.5 + r);
    pb.line_to(w_f - 1.5, h_f - 1.5 - r);
    pb.quad_to(w_f - 1.5, h_f - 1.5, w_f - 1.5 - r, h_f - 1.5);
    pb.line_to(1.5 + r, h_f - 1.5);
    pb.quad_to(1.5, h_f - 1.5, 1.5, h_f - 1.5 - r);
    pb.line_to(1.5, 1.5 + r);
    pb.quad_to(1.5, 1.5, 1.5 + r, 1.5);
    pb.close();
    let rect_path = pb.finish().unwrap();

    // Frosted smoke-taupe fill
    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(85, 80, 75, 150);
    bg_paint.anti_alias = true;
    pixmap.fill_path(&rect_path, &bg_paint, FillRule::Winding, Transform::identity(), None);

    // Soft highlight border
    let mut border_paint = Paint::default();
    border_paint.set_color_rgba8(210, 205, 195, 100);
    border_paint.anti_alias = true;
    let mut stroke = Stroke::default();
    stroke.width = 1.5;
    pixmap.stroke_path(&rect_path, &border_paint, &stroke, Transform::identity(), None);

    // 4-pointed Bézier star icon
    let cx = w_f / 2.0;
    let cy = h_f / 2.0;
    let r_outer = 16.0f32;
    let r_inner = 3.8f32;
    let mut sp = PathBuilder::new();
    sp.move_to(cx, cy - r_outer);
    sp.quad_to(cx + r_inner, cy - r_inner, cx + r_outer, cy);
    sp.quad_to(cx + r_inner, cy + r_inner, cx, cy + r_outer);
    sp.quad_to(cx - r_inner, cy + r_inner, cx - r_outer, cy);
    sp.quad_to(cx - r_inner, cy - r_inner, cx, cy - r_outer);
    sp.close();
    if let Some(star_path) = sp.finish() {
        let mut star_paint = Paint::default();
        star_paint.set_color_rgba8(255, 255, 255, 250);
        star_paint.anti_alias = true;
        pixmap.fill_path(&star_path, &star_paint, FillRule::Winding, Transform::identity(), None);
    }

    // Blit pixmap (RGBA premultiplied) → BGRA DIB → UpdateLayeredWindow
    let screen_dc = GetDC(std::ptr::null_mut());
    let mem_dc = CreateCompatibleDC(screen_dc);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB as u32,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [std::mem::zeroed(); 1],
    };

    let mut bits: *mut c_void = std::ptr::null_mut();
    let hbitmap = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    SelectObject(mem_dc, hbitmap as _);

    // RGBA → BGRA swap
    let src = pixmap.data();
    let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
    for i in 0..(w * h) as usize {
        dst[i * 4] = src[i * 4 + 2];     // B
        dst[i * 4 + 1] = src[i * 4 + 1]; // G
        dst[i * 4 + 2] = src[i * 4];     // R
        dst[i * 4 + 3] = src[i * 4 + 3]; // A
    }

    let blend = windows_sys::Win32::Graphics::Gdi::BLENDFUNCTION {
        BlendOp: 0,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: 1, // AC_SRC_ALPHA
    };

    let mut game_rect: RECT = std::mem::zeroed();
    if !state.game_hwnd.is_null() {
        GetWindowRect(state.game_hwnd, &mut game_rect);
    }
    let mut pt_dst = POINT { x: game_rect.left + OFFSET_X, y: game_rect.top + OFFSET_Y };
    let mut pt_src = POINT { x: 0, y: 0 };
    let mut size_wnd = SIZE { cx: w as i32, cy: h as i32 };

    UpdateLayeredWindow(hwnd, screen_dc, &mut pt_dst, &mut size_wnd, mem_dc, &mut pt_src, 0, &blend, ULW_ALPHA);

    DeleteObject(hbitmap as _);
    DeleteDC(mem_dc);
    ReleaseDC(std::ptr::null_mut(), screen_dc);
}

// ─────────────────────────────────────────────────────────────────────────────
// Dialog painter (opaque GDI, called from WM_PAINT when dialog is open)
// ─────────────────────────────────────────────────────────────────────────────

unsafe fn paint_dialog(hwnd: HWND, state: &UIState) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);

    // Fill background with ivory parchment
    let bg_brush = CreateSolidBrush(BG_COLOR) as *mut c_void;
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rc);
    FillRect(hdc, &rc, bg_brush);
    DeleteObject(bg_brush as _);

    SetBkMode(hdc, TRANSPARENT as _);
    let font_name: Vec<u16> = "Segoe UI\0".encode_utf16().collect();

    // ── Title ──
    let font_title = CreateFontW(22, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 2, 0, font_name.as_ptr());
    SelectObject(hdc, font_title as _);
    SetTextColor(hdc, 0x0036312B);
    let mut r_title = RECT { left: 30, top: 15, right: 350, bottom: 50 };
    let title: Vec<u16> = "Tiny Utils\0".encode_utf16().collect();
    DrawTextW(hdc, title.as_ptr(), -1, &mut r_title, (DT_SINGLELINE | DT_VCENTER) as u32);
    DeleteObject(font_title as _);

    // ── Close button [✕] ──
    let font_x = CreateFontW(20, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 2, 0, font_name.as_ptr());
    SelectObject(hdc, font_x as _);
    SetTextColor(hdc, 0x00706B60);
    let mut r_close = RECT { left: 375, top: 15, right: 405, bottom: 45 };
    let x_str: Vec<u16> = "✕\0".encode_utf16().collect();
    DrawTextW(hdc, x_str.as_ptr(), -1, &mut r_close, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
    DeleteObject(font_x as _);

    // ── Label "Pseudo:" ──
    let font_label = CreateFontW(14, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 2, 0, font_name.as_ptr());
    SelectObject(hdc, font_label as _);
    SetTextColor(hdc, 0x00706B60);
    let label_pseudo: Vec<u16> = "Pseudo:\0".encode_utf16().collect();
    let mut r_lpseudo = RECT { left: 30, top: 52, right: 200, bottom: 72 };
    DrawTextW(hdc, label_pseudo.as_ptr(), -1, &mut r_lpseudo, (DT_SINGLELINE | DT_VCENTER) as u32);

    // ── Labels "IP Address:" and "Port:" ──
    let label_ip: Vec<u16> = "IP Address:\0".encode_utf16().collect();
    let mut r_lip = RECT { left: 30, top: 110, right: 270, bottom: 130 };
    DrawTextW(hdc, label_ip.as_ptr(), -1, &mut r_lip, (DT_SINGLELINE | DT_VCENTER) as u32);
    let label_port: Vec<u16> = "Port:\0".encode_utf16().collect();
    let mut r_lport = RECT { left: 290, top: 110, right: 390, bottom: 130 };
    DrawTextW(hdc, label_port.as_ptr(), -1, &mut r_lport, (DT_SINGLELINE | DT_VCENTER) as u32);
    DeleteObject(font_label as _);

    // ── Host button (green #568062 → BGR 0x00628056) ──
    let font_btn = CreateFontW(16, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 2, 0, font_name.as_ptr());
    SelectObject(hdc, font_btn as _);
    let brush_green = CreateSolidBrush(0x00628056) as *mut c_void;
    let pen_green = CreatePen(PS_SOLID, 1, 0x00628056);
    SelectObject(hdc, brush_green as _);
    SelectObject(hdc, pen_green as _);
    RoundRect(hdc, 30, 170, 390, 210, 14, 14);
    DeleteObject(brush_green as _);
    DeleteObject(pen_green as _);
    SetTextColor(hdc, 0x00FFFFFF);
    let mut r_btn1 = RECT { left: 30, top: 170, right: 390, bottom: 210 };
    let btn1_txt: Vec<u16> = "Host Multiplayer Game\0".encode_utf16().collect();
    DrawTextW(hdc, btn1_txt.as_ptr(), -1, &mut r_btn1, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);

    // ── Join button (amber #BD7A44 → BGR 0x00447ABD) ──
    let brush_amber = CreateSolidBrush(0x00447ABD) as *mut c_void;
    let pen_amber = CreatePen(PS_SOLID, 1, 0x00447ABD);
    SelectObject(hdc, brush_amber as _);
    SelectObject(hdc, pen_amber as _);
    RoundRect(hdc, 30, 220, 390, 260, 14, 14);
    DeleteObject(brush_amber as _);
    DeleteObject(pen_amber as _);
    SetTextColor(hdc, 0x00FFFFFF);
    let mut r_btn2 = RECT { left: 30, top: 220, right: 390, bottom: 260 };
    let btn2_txt: Vec<u16> = "Join Friend\0".encode_utf16().collect();
    DrawTextW(hdc, btn2_txt.as_ptr(), -1, &mut r_btn2, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
    DeleteObject(font_btn as _);

    // ── Players section header ──
    let font_players = CreateFontW(15, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 2, 0, font_name.as_ptr());
    SelectObject(hdc, font_players as _);
    SetTextColor(hdc, 0x0036312B);
    let players_header: Vec<u16> = "Connected Players\0".encode_utf16().collect();
    let mut r_ph = RECT { left: 30, top: 270, right: 390, bottom: 290 };
    DrawTextW(hdc, players_header.as_ptr(), -1, &mut r_ph, (DT_SINGLELINE | DT_VCENTER) as u32);
    DeleteObject(font_players as _);

    // ── Player list ──
    let font_player = CreateFontW(13, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 2, 0, font_name.as_ptr());
    SelectObject(hdc, font_player as _);
    let players = state.network.get_player_list();
    let is_host = state.network.is_hosting();
    let local_name = state.network.get_local_name();

    if players.is_empty() {
        SetTextColor(hdc, 0x009A948A);
        let no_players: Vec<u16> = "No players connected\0".encode_utf16().collect();
        let mut r_np = RECT { left: 30, top: 295, right: 390, bottom: 315 };
        DrawTextW(hdc, no_players.as_ptr(), -1, &mut r_np, (DT_SINGLELINE | DT_VCENTER) as u32);
    } else {
        for (i, player) in players.iter().enumerate() {
            let y_top = PLAYER_LIST_TOP + 5 + (i as i32 * PLAYER_ROW_H);
            let y_bottom = y_top + PLAYER_ROW_H - 4;

            // Alternating row background
            if i % 2 == 0 {
                let row_brush = CreateSolidBrush(0x00F2EFE8) as *mut c_void;
                let row_rect = RECT { left: 25, top: y_top - 2, right: 395, bottom: y_bottom + 2 };
                FillRect(hdc, &row_rect, row_brush);
                DeleteObject(row_brush as _);
            }

            // Player name (with host crown)
            let display_name = if player.is_host {
                format!("★ {} (Host)", player.name)
            } else {
                player.name.clone()
            };
            let name_wide: Vec<u16> = format!("{}\0", display_name).encode_utf16().collect();
            SetTextColor(hdc, 0x0036312B);
            let mut r_name = RECT { left: 35, top: y_top, right: 300, bottom: y_bottom };
            DrawTextW(hdc, name_wide.as_ptr(), -1, &mut r_name, (DT_SINGLELINE | DT_VCENTER) as u32);

            // Kick button (only host, not for self, not for host player)
            if is_host && !player.is_host && player.name != local_name {
                let kick_wide: Vec<u16> = "[Kick]\0".encode_utf16().collect();
                SetTextColor(hdc, 0x004444CC); // red-ish
                let mut r_kick = RECT { left: 320, top: y_top, right: 385, bottom: y_bottom };
                DrawTextW(hdc, kick_wide.as_ptr(), -1, &mut r_kick, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
            }
        }
    }
    DeleteObject(font_player as _);

    // ── Status text ──
    let font_stat = CreateFontW(13, 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 2, 0, font_name.as_ptr());
    SelectObject(hdc, font_stat as _);
    SetTextColor(hdc, 0x00706B60);
    let stat: Vec<u16> = format!("● Status: {}\0", state.status_text).encode_utf16().collect();
    let mut r_stat = RECT { left: 30, top: 425, right: 390, bottom: 455 };
    DrawTextW(hdc, stat.as_ptr(), -1, &mut r_stat, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
    DeleteObject(font_stat as _);

    EndPaint(hwnd, &ps);
}

// ─────────────────────────────────────────────────────────────────────────────
// Mode switching helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Switch from layered star button → opaque GDI dialog
unsafe fn switch_to_opaque(hwnd: HWND, state: &mut UIState) {
    // 1. Remove WS_EX_LAYERED so the window becomes opaque
    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED as isize));

    // Force Windows to re-evaluate the extended style change
    SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);

    // 2. Resize + reposition to dialog dimensions
    let mut gr: RECT = std::mem::zeroed();
    if !state.game_hwnd.is_null() {
        GetWindowRect(state.game_hwnd, &mut gr);
    }
    MoveWindow(hwnd, gr.left + OFFSET_X, gr.top + OFFSET_Y, OPEN_W, OPEN_H, 0);

    // 3. Rounded corners via region
    let rgn = CreateRoundRectRgn(0, 0, OPEN_W + 1, OPEN_H + 1, 24, 24);
    SetWindowRgn(hwnd, rgn, 1); // system takes ownership of rgn

    // 4. Show EDIT controls
    ShowWindow(state.hwnd_pseudo, SW_SHOW);
    ShowWindow(state.hwnd_ip, SW_SHOW);
    ShowWindow(state.hwnd_port, SW_SHOW);

    // 5. Force repaint
    InvalidateRect(hwnd, std::ptr::null(), 1);
}

/// Switch from opaque dialog → layered star button
unsafe fn switch_to_layered(hwnd: HWND, state: &mut UIState) {
    // 1. Hide EDIT controls
    ShowWindow(state.hwnd_pseudo, SW_HIDE);
    ShowWindow(state.hwnd_ip, SW_HIDE);
    ShowWindow(state.hwnd_port, SW_HIDE);

    // 2. Remove window region
    SetWindowRgn(hwnd, std::ptr::null_mut(), 0);

    // 3. Add WS_EX_LAYERED back
    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);

    // Force Windows to re-evaluate the extended style change
    SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);

    // 4. Resize to small button
    let mut gr: RECT = std::mem::zeroed();
    if !state.game_hwnd.is_null() {
        GetWindowRect(state.game_hwnd, &mut gr);
    }
    MoveWindow(hwnd, gr.left + OFFSET_X, gr.top + OFFSET_Y, CLOSED_W, CLOSED_H, 0);

    // 5. Render the star button
    render_star_button(hwnd, state);
}

// ─────────────────────────────────────────────────────────────────────────────
// Window procedure
// ─────────────────────────────────────────────────────────────────────────────

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam as *const windows_sys::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
            let state_ptr = (*cs).lpCreateParams as isize;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr);

            let edit_class: Vec<u16> = "EDIT\0".encode_utf16().collect();
            let inst = GetModuleHandleW(std::ptr::null());

            // Pseudo input (initially hidden)
            let default_pseudo: Vec<u16> = "Builder\0".encode_utf16().collect();
            let hwnd_pseudo = CreateWindowExW(
                0,
                edit_class.as_ptr(),
                default_pseudo.as_ptr(),
                WS_CHILD | (ES_AUTOHSCROLL as u32) | WS_TABSTOP | WS_BORDER,
                30, 75, 360, 28,
                hwnd,
                ID_EDIT_PSEUDO as *mut c_void,
                inst,
                std::ptr::null(),
            );

            // IP input (initially hidden — shown when dialog opens)
            let default_ip: Vec<u16> = "127.0.0.1\0".encode_utf16().collect();
            let hwnd_ip = CreateWindowExW(
                0,
                edit_class.as_ptr(),
                default_ip.as_ptr(),
                WS_CHILD | (ES_AUTOHSCROLL as u32) | WS_TABSTOP | WS_BORDER,
                30, 133, 240, 28,
                hwnd,
                ID_EDIT_IP as *mut c_void,
                inst,
                std::ptr::null(),
            );

            // Port input (initially hidden)
            let default_port: Vec<u16> = "7777\0".encode_utf16().collect();
            let hwnd_port = CreateWindowExW(
                0,
                edit_class.as_ptr(),
                default_port.as_ptr(),
                WS_CHILD | (ES_AUTOHSCROLL as u32) | WS_TABSTOP | WS_BORDER,
                290, 133, 100, 28,
                hwnd,
                ID_EDIT_PORT as *mut c_void,
                inst,
                std::ptr::null(),
            );

            let state = &mut *(state_ptr as *mut UIState);
            state.hwnd_pseudo = hwnd_pseudo;
            state.hwnd_ip = hwnd_ip;
            state.hwnd_port = hwnd_port;
            0
        }

        WM_PAINT => {
            if IS_MENU_OPEN.load(Ordering::SeqCst) {
                let sp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UIState;
                if !sp.is_null() {
                    paint_dialog(hwnd, &*sp);
                    return 0;
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_ERASEBKGND => {
            // When dialog is open: fill with ivory to prevent black flash
            if IS_MENU_OPEN.load(Ordering::SeqCst) {
                let hdc = wparam as *mut c_void;
                let brush = CreateSolidBrush(BG_COLOR) as *mut c_void;
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rc);
                FillRect(hdc, &rc, brush);
                DeleteObject(brush as _);
                return 1;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_CTLCOLOREDIT => {
            // Set EDIT control colors: dark text on white background
            let sp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UIState;
            if !sp.is_null() {
                let state = &*sp;
                let hdc = wparam as *mut c_void;
                SetTextColor(hdc, 0x0036312B);
                SetBkColor(hdc, 0x00FFFFFF);
                return state.edit_bg_brush as LRESULT;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_SETCURSOR => {
            // Allow I-beam cursor in edit controls, arrow everywhere else
            let hit = wparam as HWND;
            let sp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UIState;
            if !sp.is_null() {
                let st = &*sp;
                if hit == st.hwnd_pseudo || hit == st.hwnd_ip || hit == st.hwnd_port {
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
            }
            let arrow = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
            SetCursor(arrow);
            1
        }

        WM_TIMER => {
            let sp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UIState;
            if !sp.is_null() {
                let st = &mut *sp;

                if !st.game_hwnd.is_null() {
                    // Show/hide based on game focus
                    let is_minimized = IsIconic(st.game_hwnd) != 0;
                    if is_minimized {
                        ShowWindow(hwnd, SW_HIDE);
                    } else {
                        let fg = GetForegroundWindow();
                        let mut fg_pid = 0u32;
                        GetWindowThreadProcessId(fg, &mut fg_pid);
                        if fg_pid == GetCurrentProcessId() {
                            ShowWindow(hwnd, SW_SHOW);
                        } else {
                            ShowWindow(hwnd, SW_HIDE);
                        }
                    }

                    // Follow game window position
                    let mut gr: RECT = std::mem::zeroed();
                    GetWindowRect(st.game_hwnd, &mut gr);
                    if gr.left != st.last_game_pos.0 || gr.top != st.last_game_pos.1 {
                        st.last_game_pos = (gr.left, gr.top);
                        let is_open = IS_MENU_OPEN.load(Ordering::SeqCst);
                        if is_open {
                            MoveWindow(hwnd, gr.left + OFFSET_X, gr.top + OFFSET_Y, OPEN_W, OPEN_H, 1);
                        } else {
                            render_star_button(hwnd, st);
                        }
                    }

                    // Refresh player list display if count changed
                    let current_count = st.network.get_player_list().len();
                    if is_menu_open_safe() && current_count != st.last_player_count {
                        st.last_player_count = current_count;
                        InvalidateRect(hwnd, std::ptr::null(), 1);
                    }
                }
            }
            0
        }

        WM_LBUTTONDOWN => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;
            let is_open = IS_MENU_OPEN.load(Ordering::SeqCst);
            let sp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UIState;

            if !is_open {
                // Click on star button → open dialog
                if !sp.is_null() {
                    IS_MENU_OPEN.store(true, Ordering::SeqCst);
                    switch_to_opaque(hwnd, &mut *sp);
                }
            } else {
                // Close [✕] button
                if x >= 375 && x <= 405 && y >= 15 && y <= 45 {
                    if !sp.is_null() {
                        IS_MENU_OPEN.store(false, Ordering::SeqCst);
                        switch_to_layered(hwnd, &mut *sp);
                    }
                    return 0;
                }

                if !sp.is_null() {
                    let state = &mut *sp;

                    // Host button (y: 170..210)
                    if x >= 30 && x <= 390 && y >= 170 && y <= 210 {
                        // Read pseudo
                        let mut pseudo_buf = [0u16; 32];
                        GetWindowTextW(state.hwnd_pseudo, pseudo_buf.as_mut_ptr(), 32);
                        let pseudo = String::from_utf16_lossy(&pseudo_buf)
                            .trim_matches('\0').trim().to_string();
                        let pseudo = if pseudo.is_empty() { "Host".to_string() } else { pseudo };
                        state.network.set_local_name(pseudo);

                        let mut port_buf = [0u16; 16];
                        GetWindowTextW(state.hwnd_port, port_buf.as_mut_ptr(), 16);
                        let port_str = String::from_utf16_lossy(&port_buf)
                            .trim_matches('\0').trim().to_string();
                        let port = port_str.parse::<u16>().unwrap_or(7777);

                        match state.network.start_host(port) {
                            Ok(_) => {
                                state.status_text = format!("Host active on port {}", port);
                                state.last_player_count = 0; // force refresh
                            }
                            Err(e) => state.status_text = format!("Error: {}", e),
                        }
                        InvalidateRect(hwnd, std::ptr::null(), 1);
                    }

                    // Join button (y: 220..260)
                    if x >= 30 && x <= 390 && y >= 220 && y <= 260 {
                        // Read pseudo
                        let mut pseudo_buf = [0u16; 32];
                        GetWindowTextW(state.hwnd_pseudo, pseudo_buf.as_mut_ptr(), 32);
                        let pseudo = String::from_utf16_lossy(&pseudo_buf)
                            .trim_matches('\0').trim().to_string();
                        let pseudo = if pseudo.is_empty() { "Guest".to_string() } else { pseudo };
                        state.network.set_local_name(pseudo);

                        let mut ip_buf = [0u16; 64];
                        let mut port_buf = [0u16; 16];
                        GetWindowTextW(state.hwnd_ip, ip_buf.as_mut_ptr(), 64);
                        GetWindowTextW(state.hwnd_port, port_buf.as_mut_ptr(), 16);

                        let ip_str = String::from_utf16_lossy(&ip_buf)
                            .trim_matches('\0').trim().to_string();
                        let port_str = String::from_utf16_lossy(&port_buf)
                            .trim_matches('\0').trim().to_string();

                        let addr = format!("{}:{}", ip_str, port_str);
                        state.status_text = format!("Connecting to {}...", addr);
                        InvalidateRect(hwnd, std::ptr::null(), 1);

                        let net_clone = Arc::clone(&state.network);
                        let hwnd_raw = hwnd as usize;
                        thread::spawn(move || unsafe {
                            let res = net_clone.connect_to_host(&addr);
                            let h = hwnd_raw as HWND;
                            let stp = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut UIState;
                            if !stp.is_null() {
                                let s = &mut *stp;
                                match res {
                                    Ok(_) => {
                                        s.status_text = "Connected successfully!".to_string();
                                        s.last_player_count = 0; // force refresh
                                    }
                                    Err(e) => s.status_text = format!("Connection failed: {}", e),
                                }
                                InvalidateRect(h, std::ptr::null(), 1);
                            }
                        });
                    }

                    // Kick buttons — check if click is in player list area
                    if y >= PLAYER_LIST_TOP && y <= PLAYER_LIST_BOTTOM && state.network.is_hosting() {
                        let players = state.network.get_player_list();
                        let local_name = state.network.get_local_name();
                        let row_index = ((y - PLAYER_LIST_TOP - 5) / PLAYER_ROW_H) as usize;

                        if row_index < players.len() {
                            let player = &players[row_index];
                            // Only kick non-host players, not self
                            if !player.is_host && player.name != local_name {
                                // Check if click is on the [Kick] area (x: 320..385)
                                if x >= 320 && x <= 385 {
                                    let kicked_name = player.name.clone();
                                    let kicked_name_clone = kicked_name.clone();
                                    let net_clone = Arc::clone(&state.network);
                                    let hwnd_raw = hwnd as usize;
                                    thread::spawn(move || unsafe {
                                        net_clone.kick_player(&kicked_name_clone);
                                        let h = hwnd_raw as HWND;
                                        let stp = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut UIState;
                                        if !stp.is_null() {
                                            (*stp).last_player_count = 0;
                                            InvalidateRect(h, std::ptr::null(), 1);
                                        }
                                    });
                                    state.status_text = format!("Kicked {}", kicked_name);
                                    InvalidateRect(hwnd, std::ptr::null(), 1);
                                }
                            }
                        }
                    }
                }
            }
            0
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn is_menu_open_safe() -> bool {
    IS_MENU_OPEN.load(Ordering::SeqCst)
}
