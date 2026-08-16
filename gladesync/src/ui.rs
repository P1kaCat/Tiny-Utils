use crate::network::NetworkManager;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    AddFontMemResourceEx, BeginPaint, CreateCompatibleDC, CreateDIBSection, CreateFontW,
    CreatePen, CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW,
    EndPaint, FillRect, GetDC, GetTextMetricsW, InvalidateRect, ReleaseDC, RoundRect,
    SelectObject, SetBkColor, SetBkMode, SetTextColor, SetWindowRgn, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, DT_CENTER, DT_SINGLELINE, DT_VCENTER,
    PAINTSTRUCT, PS_SOLID, TEXTMETRICW, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetClientRect,
    GetForegroundWindow, GetMessageW, GetWindowLongPtrW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, LoadCursorW, MoveWindow,
    PostQuitMessage, RegisterClassExW, SendMessageW, SetCursor, SetTimer, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, TranslateMessage, UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW,
    ES_AUTOHSCROLL, GWL_EXSTYLE, GWLP_USERDATA, IDC_ARROW, MSG, SW_HIDE, SW_SHOW,
    SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, ULW_ALPHA, WM_CREATE,
    WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_PAINT, WM_SETCURSOR, WM_SETFONT, WM_TIMER,
    WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

/// The game's own hand-drawn UI font, embedded so the mod's menu matches
/// Tiny Glade's native look exactly (SIL OFL licensed, see assets/PatrickHandSC-Regular.ttf).
static GAME_FONT_BYTES: &[u8] = include_bytes!("../assets/PatrickHandSC-Regular.ttf");
const GAME_FONT_NAME: &str = "Patrick Hand SC";

static IS_MENU_OPEN: AtomicBool = AtomicBool::new(false);

const ID_EDIT_PSEUDO: isize = 1000;
const ID_EDIT_IP: isize = 1001;
const ID_EDIT_PORT: isize = 1002;
const TIMER_REFRESH: usize = 1;

const OFFSET_X: i32 = 20;
const OFFSET_Y: i32 = 20;
const OPEN_W: i32 = 420;
const OPEN_H: i32 = 520;
const CLOSED_W: i32 = 56;
const CLOSED_H: i32 = 56;

/// WM_CTLCOLOREDIT = 0x0133
const WM_CTLCOLOREDIT: u32 = 0x0133;

// ── Soft cream palette (COLORREF is 0x00BBGGRR) ──
const BG_COLOR: u32 = 0x00E1F2FB;
const FIELD_BG_COLOR: u32 = 0x00F0FAFF;
const FIELD_BORDER_COLOR: u32 = 0x00BED9E6;
const TEXT_DARK: u32 = 0x0036465B;
const TEXT_MUTED: u32 = 0x00738A9C;
const ROW_ALT_COLOR: u32 = 0x00DCEEF6;
const GREEN_COLOR: u32 = 0x0086AE8F;
const AMBER_COLOR: u32 = 0x0068A8E0;
const GRAY_COLOR: u32 = 0x00C6DCE6;
const KICK_COLOR: u32 = 0x00637BC9;
const BTN_TEXT_COLOR: u32 = 0x00FFFBF3;

/// Dialog modes: 0=Idle, 1=HostSetup, 2=JoinSetup
const MODE_IDLE: u8 = 0;
const MODE_HOST: u8 = 1;
const MODE_JOIN: u8 = 2;
const MODE_HOST_ACTIVE: u8 = 3;
const MODE_JOIN_ACTIVE: u8 = 4;
const TIMER_COUNTDOWN: usize = 2;

// ── Layout constants ──
const CLOSE_L: i32 = 372;
const CLOSE_T: i32 = 10;
const CLOSE_R: i32 = 406;
const CLOSE_B: i32 = 50;

const TITLE_T: i32 = 8;
const TITLE_B: i32 = 56;

const PSEUDO_LABEL_T: i32 = 60;
const PSEUDO_LABEL_B: i32 = 82;
const PSEUDO_FIELD_X: i32 = 28;
const PSEUDO_FIELD_Y: i32 = 84;
const PSEUDO_FIELD_W: i32 = 364;
const PSEUDO_FIELD_H: i32 = 40;

const SETUP_LABEL_T: i32 = 134;
const SETUP_LABEL_B: i32 = 158;
const SETUP_FIELD_Y: i32 = 160;
const SETUP_FIELD_H: i32 = 40;

const IDLE_HOST_BTN_T: i32 = 168;
const IDLE_HOST_BTN_B: i32 = 214;
const IDLE_JOIN_BTN_T: i32 = 222;
const IDLE_JOIN_BTN_B: i32 = 268;

const SETUP_ACTION_BTN_T: i32 = 208;
const SETUP_ACTION_BTN_B: i32 = 254;
// Active (host/join) mode has no IP/port fields, so its action button
// sits higher, right under the Pseudo field, to avoid dead space.
const ACTIVE_BTN_T: i32 = 140;
const ACTIVE_BTN_B: i32 = 186;
const SETUP_BACK_BTN_T: i32 = 262;
const SETUP_BACK_BTN_B: i32 = 298;

const PLAYERS_HEADER_H: i32 = 26;
const PLAYER_ROW_H: i32 = 32;
const PLAYER_LIST_HIT_BOTTOM: i32 = 460;

const STATUS_T: i32 = 472;
const STATUS_B: i32 = 508;

struct UIState {
    network: Arc<NetworkManager>,
    game_hwnd: HWND,
    hwnd_pseudo: HWND,
    hwnd_ip: HWND,
    hwnd_port: HWND,
    edit_font: *mut c_void,
    status_text: String,
    last_game_pos: (i32, i32),
    edit_bg_brush: *mut c_void,
    last_player_count: usize,
    dialog_mode: u8,
    stop_countdown: i32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Font loading — embed the game's own TTF so no external file dependency exists
// ─────────────────────────────────────────────────────────────────────────────

unsafe fn install_game_font() {
    let mut num_fonts: u32 = 0;
    AddFontMemResourceEx(
        GAME_FONT_BYTES.as_ptr() as *const c_void,
        GAME_FONT_BYTES.len() as u32,
        std::ptr::null(),
        &mut num_fonts,
    );
}

unsafe fn make_font(size: i32, weight: i32) -> *mut c_void {
    let font_name: Vec<u16> = format!("{}\0", GAME_FONT_NAME).encode_utf16().collect();
    CreateFontW(size, 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 2, 0, font_name.as_ptr())
}

/// Position and size an EDIT control so its text is *visually* vertically
/// centered within (field_x, field_y, field_w, field_h). We measure the
/// font's true ink height (ascent+descent, excluding internal leading) —
/// hand-drawn fonts like Patrick Hand SC reserve a lot of internal leading
/// for accents/flourishes, which throws off the OS's default centering.
unsafe fn fit_edit_field(
    hwnd_edit: HWND,
    font: *mut c_void,
    field_x: i32,
    field_y: i32,
    field_w: i32,
    field_h: i32,
) {
    let hdc = GetDC(std::ptr::null_mut());
    let old_font = SelectObject(hdc, font as _);
    let mut tm: TEXTMETRICW = std::mem::zeroed();
    GetTextMetricsW(hdc, &mut tm);
    SelectObject(hdc, old_font);
    ReleaseDC(std::ptr::null_mut(), hdc);

    let ink_h = (tm.tmAscent + tm.tmDescent).max(16);
    let edit_h = (ink_h + 6).min(field_h);
    let edit_y = field_y + ((field_h - edit_h) / 2).max(0);
    let inset = 8;
    MoveWindow(
        hwnd_edit,
        field_x + inset,
        edit_y,
        (field_w - inset * 2).max(20),
        edit_h,
        1,
    );
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
        install_game_font();

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
            edit_font: std::ptr::null_mut(),
            status_text: "Ready to play".to_string(),
            last_game_pos: (game_rect.left, game_rect.top),
            edit_bg_brush: CreateSolidBrush(FIELD_BG_COLOR) as *mut c_void,
            last_player_count: 0,
            dialog_mode: MODE_IDLE,
            stop_countdown: 0,
        }));

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
// Star button renderer
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

    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(95, 85, 70, 150);
    bg_paint.anti_alias = true;
    pixmap.fill_path(&rect_path, &bg_paint, FillRule::Winding, Transform::identity(), None);

    let mut border_paint = Paint::default();
    border_paint.set_color_rgba8(230, 217, 190, 110);
    border_paint.anti_alias = true;
    let mut stroke = Stroke::default();
    stroke.width = 1.5;
    pixmap.stroke_path(&rect_path, &border_paint, &stroke, Transform::identity(), None);

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
        star_paint.set_color_rgba8(255, 250, 240, 250);
        star_paint.anti_alias = true;
        pixmap.fill_path(&star_path, &star_paint, FillRule::Winding, Transform::identity(), None);
    }

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

    let src = pixmap.data();
    let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
    for i in 0..(w * h) as usize {
        dst[i * 4] = src[i * 4 + 2];
        dst[i * 4 + 1] = src[i * 4 + 1];
        dst[i * 4 + 2] = src[i * 4];
        dst[i * 4 + 3] = src[i * 4 + 3];
    }

    let blend = windows_sys::Win32::Graphics::Gdi::BLENDFUNCTION {
        BlendOp: 0,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: 1,
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
// Helper: draw a themed input field background (rounded rect with soft border)
// ─────────────────────────────────────────────────────────────────────────────

unsafe fn draw_field_bg(hdc: *mut c_void, x: i32, y: i32, w: i32, h: i32) {
    let brush = CreateSolidBrush(FIELD_BG_COLOR) as *mut c_void;
    let pen = CreatePen(PS_SOLID, 1, FIELD_BORDER_COLOR);
    SelectObject(hdc, brush as _);
    SelectObject(hdc, pen as _);
    RoundRect(hdc, x, y, x + w, y + h, 10, 10);
    DeleteObject(brush as _);
    DeleteObject(pen as _);
}

// ─────────────────────────────────────────────────────────────────────────────
// Dialog painter
// ─────────────────────────────────────────────────────────────────────────────

unsafe fn paint_dialog(hwnd: HWND, state: &UIState) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);

    let bg_brush = CreateSolidBrush(BG_COLOR) as *mut c_void;
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rc);
    FillRect(hdc, &rc, bg_brush);
    DeleteObject(bg_brush as _);

    SetBkMode(hdc, TRANSPARENT as _);

    // ── Title ──
    let font_title = make_font(32, 400);
    SelectObject(hdc, font_title as _);
    SetTextColor(hdc, TEXT_DARK);
    let mut r_title = RECT { left: 30, top: TITLE_T, right: 350, bottom: TITLE_B };
    let title: Vec<u16> = "Tiny Utils\0".encode_utf16().collect();
    DrawTextW(hdc, title.as_ptr(), -1, &mut r_title, (DT_SINGLELINE | DT_VCENTER) as u32);
    DeleteObject(font_title as _);

    // ── Close [✕] ──
    let font_x = make_font(24, 400);
    SelectObject(hdc, font_x as _);
    SetTextColor(hdc, TEXT_MUTED);
    let mut r_close = RECT { left: CLOSE_L, top: CLOSE_T, right: CLOSE_R, bottom: CLOSE_B };
    let x_str: Vec<u16> = "✕\0".encode_utf16().collect();
    DrawTextW(hdc, x_str.as_ptr(), -1, &mut r_close, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
    DeleteObject(font_x as _);

    // ── "Pseudo:" label ──
    let font_label = make_font(21, 400);
    SelectObject(hdc, font_label as _);
    SetTextColor(hdc, TEXT_MUTED);
    let label_pseudo: Vec<u16> = "Pseudo:\0".encode_utf16().collect();
    let mut r_lpseudo = RECT { left: 30, top: PSEUDO_LABEL_T, right: 200, bottom: PSEUDO_LABEL_B };
    DrawTextW(hdc, label_pseudo.as_ptr(), -1, &mut r_lpseudo, (DT_SINGLELINE | DT_VCENTER) as u32);

    // ── Pseudo field background ──
    draw_field_bg(hdc, PSEUDO_FIELD_X, PSEUDO_FIELD_Y, PSEUDO_FIELD_W, PSEUDO_FIELD_H);

    // ── IP/Port labels (only in setup modes) ──
    if state.dialog_mode == MODE_HOST {
        let label_port: Vec<u16> = "Port:\0".encode_utf16().collect();
        let mut r_lport = RECT { left: 100, top: SETUP_LABEL_T, right: 320, bottom: SETUP_LABEL_B };
        DrawTextW(hdc, label_port.as_ptr(), -1, &mut r_lport, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
        draw_field_bg(hdc, 108, SETUP_FIELD_Y, 204, SETUP_FIELD_H);
    } else if state.dialog_mode == MODE_JOIN {
        let label_ip: Vec<u16> = "IP Address:\0".encode_utf16().collect();
        let mut r_lip = RECT { left: 30, top: SETUP_LABEL_T, right: 270, bottom: SETUP_LABEL_B };
        DrawTextW(hdc, label_ip.as_ptr(), -1, &mut r_lip, (DT_SINGLELINE | DT_VCENTER) as u32);
        let label_port: Vec<u16> = "Port:\0".encode_utf16().collect();
        let mut r_lport = RECT { left: 290, top: SETUP_LABEL_T, right: 390, bottom: SETUP_LABEL_B };
        DrawTextW(hdc, label_port.as_ptr(), -1, &mut r_lport, (DT_SINGLELINE | DT_VCENTER) as u32);
        draw_field_bg(hdc, 28, SETUP_FIELD_Y, 244, SETUP_FIELD_H);
        draw_field_bg(hdc, 288, SETUP_FIELD_Y, 104, SETUP_FIELD_H);
    }
    DeleteObject(font_label as _);

    // ── Buttons ──
    let font_btn = make_font(23, 400);
    SelectObject(hdc, font_btn as _);

    match state.dialog_mode {
        MODE_IDLE => {
            let brush_green = CreateSolidBrush(GREEN_COLOR) as *mut c_void;
            let pen_green = CreatePen(PS_SOLID, 1, GREEN_COLOR);
            SelectObject(hdc, brush_green as _);
            SelectObject(hdc, pen_green as _);
            RoundRect(hdc, 30, IDLE_HOST_BTN_T, 390, IDLE_HOST_BTN_B, 16, 16);
            DeleteObject(brush_green as _);
            DeleteObject(pen_green as _);
            SetTextColor(hdc, BTN_TEXT_COLOR);
            let mut r_btn1 = RECT { left: 30, top: IDLE_HOST_BTN_T, right: 390, bottom: IDLE_HOST_BTN_B };
            let btn1_txt: Vec<u16> = "Host Multiplayer Game\0".encode_utf16().collect();
            DrawTextW(hdc, btn1_txt.as_ptr(), -1, &mut r_btn1, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);

            let brush_amber = CreateSolidBrush(AMBER_COLOR) as *mut c_void;
            let pen_amber = CreatePen(PS_SOLID, 1, AMBER_COLOR);
            SelectObject(hdc, brush_amber as _);
            SelectObject(hdc, pen_amber as _);
            RoundRect(hdc, 30, IDLE_JOIN_BTN_T, 390, IDLE_JOIN_BTN_B, 16, 16);
            DeleteObject(brush_amber as _);
            DeleteObject(pen_amber as _);
            SetTextColor(hdc, BTN_TEXT_COLOR);
            let mut r_btn2 = RECT { left: 30, top: IDLE_JOIN_BTN_T, right: 390, bottom: IDLE_JOIN_BTN_B };
            let btn2_txt: Vec<u16> = "Join Friend\0".encode_utf16().collect();
            DrawTextW(hdc, btn2_txt.as_ptr(), -1, &mut r_btn2, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
        }
        MODE_HOST | MODE_JOIN => {
            let (brush_color, label) = if state.dialog_mode == MODE_HOST {
                (GREEN_COLOR, "▶ Start Hosting\0")
            } else {
                (AMBER_COLOR, "▶ Connect\0")
            };
            let brush_action = CreateSolidBrush(brush_color) as *mut c_void;
            let pen_action = CreatePen(PS_SOLID, 1, brush_color);
            SelectObject(hdc, brush_action as _);
            SelectObject(hdc, pen_action as _);
            RoundRect(hdc, 30, SETUP_ACTION_BTN_T, 390, SETUP_ACTION_BTN_B, 16, 16);
            DeleteObject(brush_action as _);
            DeleteObject(pen_action as _);
            SetTextColor(hdc, BTN_TEXT_COLOR);
            let mut r_btn1 = RECT { left: 30, top: SETUP_ACTION_BTN_T, right: 390, bottom: SETUP_ACTION_BTN_B };
            let btn1_txt: Vec<u16> = label.encode_utf16().collect();
            DrawTextW(hdc, btn1_txt.as_ptr(), -1, &mut r_btn1, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);

            let font_back = make_font(19, 400);
            SelectObject(hdc, font_back as _);
            let brush_gray = CreateSolidBrush(GRAY_COLOR) as *mut c_void;
            let pen_gray = CreatePen(PS_SOLID, 1, GRAY_COLOR);
            SelectObject(hdc, brush_gray as _);
            SelectObject(hdc, pen_gray as _);
            RoundRect(hdc, 30, SETUP_BACK_BTN_T, 390, SETUP_BACK_BTN_B, 12, 12);
            DeleteObject(brush_gray as _);
            DeleteObject(pen_gray as _);
            SetTextColor(hdc, TEXT_DARK);
            let mut r_back = RECT { left: 30, top: SETUP_BACK_BTN_T, right: 390, bottom: SETUP_BACK_BTN_B };
            let back_txt: Vec<u16> = "← Back\0".encode_utf16().collect();
            DrawTextW(hdc, back_txt.as_ptr(), -1, &mut r_back, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
            SelectObject(hdc, font_btn as _);
            DeleteObject(font_back as _);
        }
        MODE_HOST_ACTIVE => {
            let (btn_color, btn_label): (u32, String) = if state.stop_countdown > 0 {
                (GRAY_COLOR, format!("Stop Hosting... {}\0", state.stop_countdown))
            } else if state.stop_countdown == -1 {
                (0x00A84D4D, "Confirm Stop?\0".to_string())
            } else {
                (KICK_COLOR, "Stop Hosting\0".to_string())
            };
            let brush_action = CreateSolidBrush(btn_color) as *mut c_void;
            let pen_action = CreatePen(PS_SOLID, 1, btn_color);
            SelectObject(hdc, brush_action as _);
            SelectObject(hdc, pen_action as _);
            RoundRect(hdc, 30, ACTIVE_BTN_T, 390, ACTIVE_BTN_B, 16, 16);
            DeleteObject(brush_action as _);
            DeleteObject(pen_action as _);
            SetTextColor(hdc, BTN_TEXT_COLOR);
            let mut r_btn1 = RECT { left: 30, top: ACTIVE_BTN_T, right: 390, bottom: ACTIVE_BTN_B };
            let btn1_txt: Vec<u16> = btn_label.encode_utf16().collect();
            DrawTextW(hdc, btn1_txt.as_ptr(), -1, &mut r_btn1, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
        }
        MODE_JOIN_ACTIVE => {
            let brush_action = CreateSolidBrush(KICK_COLOR) as *mut c_void;
            let pen_action = CreatePen(PS_SOLID, 1, KICK_COLOR);
            SelectObject(hdc, brush_action as _);
            SelectObject(hdc, pen_action as _);
            RoundRect(hdc, 30, ACTIVE_BTN_T, 390, ACTIVE_BTN_B, 16, 16);
            DeleteObject(brush_action as _);
            DeleteObject(pen_action as _);
            SetTextColor(hdc, BTN_TEXT_COLOR);
            let mut r_btn1 = RECT { left: 30, top: ACTIVE_BTN_T, right: 390, bottom: ACTIVE_BTN_B };
            let btn1_txt: Vec<u16> = "Leave\0".encode_utf16().collect();
            DrawTextW(hdc, btn1_txt.as_ptr(), -1, &mut r_btn1, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
        }
        _ => {}
    }
    DeleteObject(font_btn as _);

    // ── Players section header ──
    let header_y = match state.dialog_mode {
        MODE_IDLE => IDLE_JOIN_BTN_B + 14,
        MODE_HOST_ACTIVE | MODE_JOIN_ACTIVE => ACTIVE_BTN_B + 14,
        _ => SETUP_BACK_BTN_B + 14,
    };
    let font_players = make_font(21, 400);
    SelectObject(hdc, font_players as _);
    SetTextColor(hdc, TEXT_DARK);
    let players_header: Vec<u16> = "Connected Players\0".encode_utf16().collect();
    let mut r_ph = RECT { left: 30, top: header_y, right: 390, bottom: header_y + PLAYERS_HEADER_H };
    DrawTextW(hdc, players_header.as_ptr(), -1, &mut r_ph, (DT_SINGLELINE | DT_VCENTER) as u32);
    DeleteObject(font_players as _);

    // ── Player list ──
    let font_player = make_font(19, 400);
    SelectObject(hdc, font_player as _);
    let players = state.network.get_player_list();
    let is_host = state.network.is_hosting();
    let local_name = state.network.get_local_name();
    let list_top = header_y + PLAYERS_HEADER_H + 8;

    if players.is_empty() {
        SetTextColor(hdc, TEXT_MUTED);
        let no_players: Vec<u16> = "No players connected\0".encode_utf16().collect();
        let mut r_np = RECT { left: 30, top: list_top, right: 390, bottom: list_top + 24 };
        DrawTextW(hdc, no_players.as_ptr(), -1, &mut r_np, (DT_SINGLELINE | DT_VCENTER) as u32);
    } else {
        for (i, player) in players.iter().enumerate() {
            let y_top = list_top + (i as i32 * PLAYER_ROW_H);
            let y_bottom = y_top + PLAYER_ROW_H - 4;

            if i % 2 == 0 {
                let row_brush = CreateSolidBrush(ROW_ALT_COLOR) as *mut c_void;
                let row_rect = RECT { left: 25, top: y_top - 2, right: 395, bottom: y_bottom + 2 };
                FillRect(hdc, &row_rect, row_brush);
                DeleteObject(row_brush as _);
            }

            let display_name = if player.is_host {
                format!("★ {} (Host)", player.name)
            } else {
                player.name.clone()
            };
            let name_wide: Vec<u16> = format!("{}\0", display_name).encode_utf16().collect();
            SetTextColor(hdc, TEXT_DARK);
            let mut r_name = RECT { left: 35, top: y_top, right: 295, bottom: y_bottom };
            DrawTextW(hdc, name_wide.as_ptr(), -1, &mut r_name, (DT_SINGLELINE | DT_VCENTER) as u32);

            if is_host && !player.is_host && player.name != local_name {
                let kick_wide: Vec<u16> = "[Kick]\0".encode_utf16().collect();
                SetTextColor(hdc, KICK_COLOR);
                let mut r_kick = RECT { left: 300, top: y_top, right: 390, bottom: y_bottom };
                DrawTextW(hdc, kick_wide.as_ptr(), -1, &mut r_kick, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
            }
        }
    }
    DeleteObject(font_player as _);

    // ── Status text ──
    let font_stat = make_font(18, 400);
    SelectObject(hdc, font_stat as _);
    SetTextColor(hdc, TEXT_MUTED);
    let stat: Vec<u16> = format!("● Status: {}\0", state.status_text).encode_utf16().collect();
    let mut r_stat = RECT { left: 30, top: STATUS_T, right: 390, bottom: STATUS_B };
    DrawTextW(hdc, stat.as_ptr(), -1, &mut r_stat, (DT_CENTER | DT_SINGLELINE | DT_VCENTER) as u32);
    DeleteObject(font_stat as _);

    EndPaint(hwnd, &ps);
}

// ─────────────────────────────────────────────────────────────────────────────
// Mode switching helpers
// ─────────────────────────────────────────────────────────────────────────────

unsafe fn switch_to_opaque(hwnd: HWND, state: &mut UIState) {
    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED as isize));
    SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);

    let mut gr: RECT = std::mem::zeroed();
    if !state.game_hwnd.is_null() {
        GetWindowRect(state.game_hwnd, &mut gr);
    }
    MoveWindow(hwnd, gr.left + OFFSET_X, gr.top + OFFSET_Y, OPEN_W, OPEN_H, 0);

    let rgn = CreateRoundRectRgn(0, 0, OPEN_W + 1, OPEN_H + 1, 24, 24);
    SetWindowRgn(hwnd, rgn, 1);

    ShowWindow(state.hwnd_pseudo, SW_SHOW);
    update_field_visibility(state);

    InvalidateRect(hwnd, std::ptr::null(), 1);
}

unsafe fn switch_to_layered(hwnd: HWND, state: &mut UIState) {
    ShowWindow(state.hwnd_pseudo, SW_HIDE);
    ShowWindow(state.hwnd_ip, SW_HIDE);
    ShowWindow(state.hwnd_port, SW_HIDE);

    // Preserve active mode if still hosting or connected
    if !state.network.is_active() {
        state.dialog_mode = MODE_IDLE;
    }

    SetWindowRgn(hwnd, std::ptr::null_mut(), 0);

    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
    SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);

    let mut gr: RECT = std::mem::zeroed();
    if !state.game_hwnd.is_null() {
        GetWindowRect(state.game_hwnd, &mut gr);
    }
    MoveWindow(hwnd, gr.left + OFFSET_X, gr.top + OFFSET_Y, CLOSED_W, CLOSED_H, 0);

    render_star_button(hwnd, state);
}

unsafe fn update_field_visibility(state: &mut UIState) {
    match state.dialog_mode {
        MODE_IDLE | MODE_HOST_ACTIVE | MODE_JOIN_ACTIVE => {
            ShowWindow(state.hwnd_ip, SW_HIDE);
            ShowWindow(state.hwnd_port, SW_HIDE);
        }
        MODE_HOST => {
            ShowWindow(state.hwnd_ip, SW_HIDE);
            fit_edit_field(state.hwnd_port, state.edit_font, 108, SETUP_FIELD_Y, 204, SETUP_FIELD_H);
            ShowWindow(state.hwnd_port, SW_SHOW);
        }
        MODE_JOIN => {
            fit_edit_field(state.hwnd_ip, state.edit_font, 28, SETUP_FIELD_Y, 244, SETUP_FIELD_H);
            fit_edit_field(state.hwnd_port, state.edit_font, 288, SETUP_FIELD_Y, 104, SETUP_FIELD_H);
            ShowWindow(state.hwnd_ip, SW_SHOW);
            ShowWindow(state.hwnd_port, SW_SHOW);
        }
        _ => {}
    }
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

            let default_pseudo: Vec<u16> = "Builder\0".encode_utf16().collect();
            let hwnd_pseudo = CreateWindowExW(
                0,
                edit_class.as_ptr(),
                default_pseudo.as_ptr(),
                WS_CHILD | (ES_AUTOHSCROLL as u32) | WS_TABSTOP,
                PSEUDO_FIELD_X, PSEUDO_FIELD_Y, PSEUDO_FIELD_W, PSEUDO_FIELD_H,
                hwnd,
                ID_EDIT_PSEUDO as *mut c_void,
                inst,
                std::ptr::null(),
            );

            let default_ip: Vec<u16> = "127.0.0.1\0".encode_utf16().collect();
            let hwnd_ip = CreateWindowExW(
                0,
                edit_class.as_ptr(),
                default_ip.as_ptr(),
                WS_CHILD | (ES_AUTOHSCROLL as u32) | WS_TABSTOP,
                28, SETUP_FIELD_Y, 244, SETUP_FIELD_H,
                hwnd,
                ID_EDIT_IP as *mut c_void,
                inst,
                std::ptr::null(),
            );

            let default_port: Vec<u16> = "7777\0".encode_utf16().collect();
            let hwnd_port = CreateWindowExW(
                0,
                edit_class.as_ptr(),
                default_port.as_ptr(),
                WS_CHILD | (ES_AUTOHSCROLL as u32) | WS_TABSTOP,
                288, SETUP_FIELD_Y, 104, SETUP_FIELD_H,
                hwnd,
                ID_EDIT_PORT as *mut c_void,
                inst,
                std::ptr::null(),
            );

            // Apply the game font, then precisely re-fit each field so text
            // is visually centered (compensates for the font's internal leading).
            let edit_font = make_font(21, 400);
            SendMessageW(hwnd_pseudo, WM_SETFONT, edit_font as usize, 1);
            SendMessageW(hwnd_ip, WM_SETFONT, edit_font as usize, 1);
            SendMessageW(hwnd_port, WM_SETFONT, edit_font as usize, 1);

            let state = &mut *(state_ptr as *mut UIState);
            state.hwnd_pseudo = hwnd_pseudo;
            state.hwnd_ip = hwnd_ip;
            state.hwnd_port = hwnd_port;
            state.edit_font = edit_font;

            fit_edit_field(hwnd_pseudo, edit_font, PSEUDO_FIELD_X, PSEUDO_FIELD_Y, PSEUDO_FIELD_W, PSEUDO_FIELD_H);
            fit_edit_field(hwnd_ip, edit_font, 28, SETUP_FIELD_Y, 244, SETUP_FIELD_H);
            fit_edit_field(hwnd_port, edit_font, 288, SETUP_FIELD_Y, 104, SETUP_FIELD_H);
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
            let sp = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UIState;
            if !sp.is_null() {
                let state = &*sp;
                let hdc = wparam as *mut c_void;
                SetTextColor(hdc, TEXT_DARK);
                SetBkColor(hdc, FIELD_BG_COLOR);
                return state.edit_bg_brush as LRESULT;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_SETCURSOR => {
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

                if wparam == TIMER_COUNTDOWN {
                    if st.stop_countdown > 0 {
                        st.stop_countdown -= 1;
                        if st.stop_countdown == 0 {
                            st.stop_countdown = -1;
                        }
                        InvalidateRect(hwnd, std::ptr::null(), 1);
                    }
                    return 0;
                }

                if !st.game_hwnd.is_null() {
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

                    let current_count = st.network.get_player_list().len();
                    if IS_MENU_OPEN.load(Ordering::SeqCst) && current_count != st.last_player_count {
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
                if !sp.is_null() {
                    IS_MENU_OPEN.store(true, Ordering::SeqCst);
                    switch_to_opaque(hwnd, &mut *sp);
                }
            } else {
                if x >= CLOSE_L && x <= CLOSE_R && y >= CLOSE_T && y <= CLOSE_B {
                    if !sp.is_null() {
                        IS_MENU_OPEN.store(false, Ordering::SeqCst);
                        switch_to_layered(hwnd, &mut *sp);
                    }
                    return 0;
                }

                if !sp.is_null() {
                    let state = &mut *sp;

                    match state.dialog_mode {
                        MODE_IDLE => {
                            if x >= 30 && x <= 390 && y >= IDLE_HOST_BTN_T && y <= IDLE_HOST_BTN_B {
                                state.dialog_mode = MODE_HOST;
                                update_field_visibility(state);
                                InvalidateRect(hwnd, std::ptr::null(), 1);
                            } else if x >= 30 && x <= 390 && y >= IDLE_JOIN_BTN_T && y <= IDLE_JOIN_BTN_B {
                                state.dialog_mode = MODE_JOIN;
                                update_field_visibility(state);
                                InvalidateRect(hwnd, std::ptr::null(), 1);
                            } else {
                                let header_y = IDLE_JOIN_BTN_B + 14;
                                let list_top = header_y + PLAYERS_HEADER_H + 8;
                                if y >= list_top && y <= PLAYER_LIST_HIT_BOTTOM && state.network.is_hosting() {
                                    handle_kick_click(hwnd, state, x, y, list_top);
                                }
                            }
                        }
                        MODE_HOST => {
                            if x >= 30 && x <= 390 && y >= SETUP_ACTION_BTN_T && y <= SETUP_ACTION_BTN_B {
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
                                        state.last_player_count = 0;
                                        state.dialog_mode = MODE_HOST_ACTIVE;
                                        update_field_visibility(state);
                                    }
                                    Err(e) => state.status_text = format!("Error: {}", e),
                                }
                                InvalidateRect(hwnd, std::ptr::null(), 1);
                            } else if x >= 30 && x <= 390 && y >= SETUP_BACK_BTN_T && y <= SETUP_BACK_BTN_B {
                                state.dialog_mode = MODE_IDLE;
                                update_field_visibility(state);
                                InvalidateRect(hwnd, std::ptr::null(), 1);
                            }
                        }
                        MODE_JOIN => {
                            if x >= 30 && x <= 390 && y >= SETUP_ACTION_BTN_T && y <= SETUP_ACTION_BTN_B {
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
                                                s.last_player_count = 0;
                                                s.dialog_mode = MODE_JOIN_ACTIVE;
                                                update_field_visibility(&mut *s);
                                            }
                                            Err(e) => s.status_text = format!("Connection failed: {}", e),
                                        }
                                        InvalidateRect(h, std::ptr::null(), 1);
                                    }
                                });
                            } else if x >= 30 && x <= 390 && y >= SETUP_BACK_BTN_T && y <= SETUP_BACK_BTN_B {
                                state.dialog_mode = MODE_IDLE;
                                update_field_visibility(state);
                                InvalidateRect(hwnd, std::ptr::null(), 1);
                            }
                        }
                        MODE_HOST_ACTIVE => {
                            if x >= 30 && x <= 390 && y >= ACTIVE_BTN_T && y <= ACTIVE_BTN_B {
                                if state.stop_countdown == 0 {
                                    state.stop_countdown = 5;
                                    SetTimer(hwnd, TIMER_COUNTDOWN, 1000, None);
                                    InvalidateRect(hwnd, std::ptr::null(), 1);
                                } else if state.stop_countdown == -1 {
                                    state.network.disconnect();
                                    state.stop_countdown = 0;
                                    state.status_text = "Hosting stopped".to_string();
                                    state.dialog_mode = MODE_IDLE;
                                    update_field_visibility(state);
                                    InvalidateRect(hwnd, std::ptr::null(), 1);
                                }
                            }
                            let header_y = ACTIVE_BTN_B + 14;
                            let list_top = header_y + PLAYERS_HEADER_H + 8;
                            if y >= list_top && y <= PLAYER_LIST_HIT_BOTTOM && state.network.is_hosting() {
                                handle_kick_click(hwnd, state, x, y, list_top);
                            }
                        }
                        MODE_JOIN_ACTIVE => {
                            if x >= 30 && x <= 390 && y >= ACTIVE_BTN_T && y <= ACTIVE_BTN_B {
                                state.network.disconnect();
                                state.status_text = "Disconnected".to_string();
                                state.dialog_mode = MODE_IDLE;
                                update_field_visibility(state);
                                InvalidateRect(hwnd, std::ptr::null(), 1);
                            }
                        }
                        _ => {}
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

unsafe fn handle_kick_click(hwnd: HWND, state: &mut UIState, x: i32, y: i32, list_top: i32) {
    let players = state.network.get_player_list();
    let local_name = state.network.get_local_name();
    let row_index = ((y - list_top) / PLAYER_ROW_H) as usize;

    if row_index < players.len() {
        let player = &players[row_index];
        if !player.is_host && player.name != local_name {
            if x >= 300 && x <= 390 {
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
