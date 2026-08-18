use crate::network::NetworkManager;
use crate::protocol::{ActionPayload, EditCategory, NetMessage};
use base64::Engine;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, GetAsyncKeyState, KEYEVENTF_KEYUP};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PostMessageW, SetForegroundWindow, WM_KEYDOWN, WM_KEYUP,
};
use windows_sys::Win32::System::Console::{
    AllocConsole, FreeConsole, GetStdHandle, SetConsoleTitleW,
    STD_OUTPUT_HANDLE, WriteConsoleW,
};
use std::sync::Mutex;

pub static BORDER_UNLOCKED: AtomicBool = AtomicBool::new(false);

// ─────────────────────────────────────────────────────────────────────────────
// Debug console — F12 opens a console window showing all GladeSync log
// messages. A ring buffer stores the last 500 messages so nothing is lost
// before the console is opened.
// ─────────────────────────────────────────────────────────────────────────────

static CONSOLE_OPEN: AtomicBool = AtomicBool::new(false);
static LOG_BUFFER: Mutex<Vec<String>> = Mutex::new(Vec::new());
const MAX_LOG_ENTRIES: usize = 500;

/// Log a message. Stored in the ring buffer and written to the console if
/// it's open. Use this instead of println! for anything you want to see
/// in the debug console.
pub fn log_msg(msg: &str) {
    if let Ok(mut buf) = LOG_BUFFER.lock() {
        buf.push(msg.to_string());
        if buf.len() > MAX_LOG_ENTRIES {
            buf.remove(0);
        }
    }
    if CONSOLE_OPEN.load(Ordering::SeqCst) {
        write_console_line(msg);
    }
}

#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        $crate::hook::log_msg(&format!($($arg)*))
    };
}

fn write_console_line(msg: &str) {
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        if h.is_null() || (h as usize) == usize::MAX {
            return;
        }
        let line = format!("{}\r\n", msg);
        let utf16: Vec<u16> = line.encode_utf16().collect();
        let mut written = 0u32;
        WriteConsoleW(h, utf16.as_ptr(), utf16.len() as u32, &mut written, std::ptr::null());
    }
}

/// Toggle the debug console window. F12 opens/closes it.
pub fn toggle_debug_console() {
    if CONSOLE_OPEN.load(Ordering::SeqCst) {
        write_console_line("[GladeSync] Closing console...");
        unsafe { FreeConsole() };
        CONSOLE_OPEN.store(false, Ordering::SeqCst);
    } else {
        unsafe {
            if AllocConsole() != 0 {
                let title: Vec<u16> = "GladeSync Debug\0".encode_utf16().collect();
                SetConsoleTitleW(title.as_ptr());

                CONSOLE_OPEN.store(true, Ordering::SeqCst);

                write_console_line("========================================");
                write_console_line("  GladeSync Debug Console");
                write_console_line("  F12 = close | F10 = manual sync");
                write_console_line("========================================");
                write_console_line("");

                // Dump the log buffer
                if let Ok(buf) = LOG_BUFFER.lock() {
                    for entry in buf.iter() {
                        write_console_line(entry);
                    }
                }

                write_console_line("");
                write_console_line("--- End of log buffer (new messages below) ---");
            }
        }
    }
}

/// Virtual key code used to trigger auto-reload when a save is received.
/// F9 = 0x78 (common quick-load key in many games).
/// Change this if Tiny Glade uses a different key.
const RELOAD_VK: u8 = 0x78; // F9

/// Virtual key code for manual sync trigger.
/// F10 = 0x79 (host presses this to force-send their save state).
const MANUAL_SYNC_VK: i32 = 0x79; // F10

/// Virtual key code for toggling the debug console.
const VK_F12: i32 = 0x7B; // F12

// ─────────────────────────────────────────────────────────────────────────────
// Game window handle — stored globally so the network thread can access it
// to send keypresses for auto-reload. Set by the UI thread after the game
// window is found.
// ─────────────────────────────────────────────────────────────────────────────

static GAME_HWND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

pub fn set_game_hwnd(hwnd: HWND) {
    GAME_HWND.store(hwnd as *mut c_void, Ordering::SeqCst);
    debug_log!("[GladeSync] Game window handle stored: {:p}", hwnd);
}

fn get_game_hwnd() -> HWND {
    GAME_HWND.load(Ordering::SeqCst) as HWND
}

/// Trigger a quick-load in the game by simulating an F9 keypress.
/// Called automatically when a client receives a save state from the host.
///
/// Uses two methods simultaneously to maximize compatibility:
/// 1. `keybd_event` — sends to the OS foreground window (works if the game
///    is focused, which it usually is during play).
/// 2. `PostMessageW` — sends directly to the game window's message queue
///    (works even if the mod's UI overlay has focus).
pub fn trigger_reload() {
    let hwnd = get_game_hwnd();

    // Try to bring the game window to the foreground first
    if !hwnd.is_null() {
        unsafe {
            SetForegroundWindow(hwnd);
            thread::sleep(Duration::from_millis(100));
        }
    }

    // Method 1: keybd_event (OS-level input simulation)
    unsafe {
        keybd_event(RELOAD_VK, 0, 0, 0);
        thread::sleep(Duration::from_millis(50));
        keybd_event(RELOAD_VK, 0, KEYEVENTF_KEYUP, 0);
    }

    // Method 2: PostMessageW directly to the game window
    if !hwnd.is_null() {
        unsafe {
            PostMessageW(hwnd, WM_KEYDOWN, RELOAD_VK as usize, 0);
            thread::sleep(Duration::from_millis(50));
            PostMessageW(hwnd, WM_KEYUP, RELOAD_VK as usize, 0);
        }
    }

    debug_log!("[GladeSync] Reload keypress sent (F9/VK=0x{:X})", RELOAD_VK);
}

pub struct HookEngine {
    base_address: usize,
    network: Arc<NetworkManager>,
}

impl HookEngine {
    pub fn new(network: Arc<NetworkManager>) -> Self {
        let base_address = unsafe { GetModuleHandleA(std::ptr::null()) as usize };
        Self { base_address, network }
    }

    pub fn start(&self) {
        println!("[GladeSync Engine] Game Base Address: 0x{:X}", self.base_address);

        let net = Arc::clone(&self.network);
        thread::spawn(move || {
            // ── Real-time save state sync ──
            let mut last_save_mtime: Option<SystemTime> = None;
            let mut last_sync_time: Option<SystemTime> = None;
            let min_sync_interval = Duration::from_millis(1500);
            let mut save_dir_logged = false;
            let mut f10_was_down = false;
            let mut f12_was_down = false;

            loop {
                thread::sleep(Duration::from_millis(300));

                // ── F10: manual sync (host can press F10 to force-send save) ──
                let f10_down = (unsafe { GetAsyncKeyState(MANUAL_SYNC_VK) } as u16) & 0x8000 != 0;
                if f10_down && !f10_was_down {
                    if !net.is_hosting() {
                        debug_log!("[GladeSync] F10 pressed but not hosting — sync ignored");
                    } else {
                        debug_log!("[GladeSync] F10 pressed — manual sync triggered");
                        match read_latest_save() {
                            Some((fname, bytes)) => {
                                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                let msg = NetMessage::SyncSaveState {
                                    glade_name: fname,
                                    save_bytes_base64: b64,
                                };
                                net.broadcast_message(&msg);
                                last_sync_time = Some(SystemTime::now());
                                last_save_mtime = None;
                                debug_log!("[GladeSync] Manual sync: sent save state ({} bytes)", bytes.len());
                            }
                            None => {
                                debug_log!("[GladeSync] F10: no save file found!");
                                if let Some(dir) = find_save_dir() {
                                    debug_log!("[GladeSync] Save dir detected: {}", dir.display());
                                } else {
                                    debug_log!("[GladeSync] Save dir NOT found");
                                }
                            }
                        }
                    }
                }
                f10_was_down = f10_down;

                // ── F12: toggle debug console ──
                let f12_down = (unsafe { GetAsyncKeyState(VK_F12) } as u16) & 0x8000 != 0;
                if f12_down && !f12_was_down {
                    toggle_debug_console();
                }
                f12_was_down = f12_down;

                if !net.is_hosting() {
                    continue;
                }

                // Log save dir status once when hosting starts
                if !save_dir_logged {
                    save_dir_logged = true;
                    match find_save_dir() {
                        Some(dir) => {
                            debug_log!("[GladeSync] Save dir: {}", dir.display());
                            match get_latest_save_info() {
                                Some((name, mtime, size)) => {
                                    debug_log!("[GladeSync] Latest save: {} ({} bytes, mtime={:?})", name, size, mtime);
                                }
                                None => debug_log!("[GladeSync] No save files found in dir"),
                            }
                        }
                        None => debug_log!("[GladeSync] WARNING: save dir not found! Save sync disabled."),
                    }
                }

                // Rate limit
                if let Some(last) = last_sync_time {
                    if SystemTime::now().duration_since(last).unwrap_or_default() < min_sync_interval {
                        continue;
                    }
                }

                // Auto-sync: check if save file changed
                if let Some((_, mtime, _)) = get_latest_save_info() {
                    let changed = match last_save_mtime {
                        None => true,
                        Some(last) => mtime > last,
                    };

                    if changed {
                        last_save_mtime = Some(mtime);
                        last_sync_time = Some(SystemTime::now());

                        if let Some((fname, bytes)) = read_latest_save() {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                            let msg = NetMessage::SyncSaveState {
                                glade_name: fname,
                                save_bytes_base64: b64,
                            };
                            net.broadcast_message(&msg);
                            println!(
                                "[GladeSync] Auto-sync: sent save ({} bytes)",
                                bytes.len()
                            );
                        }
                    }
                }
            }
        });
    }

    pub fn on_local_edit(&self, category: EditCategory, hex_data: String) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let payload = ActionPayload {
            edit_category: category, action_id: now, timestamp: now, data_hex: hex_data,
        };
        self.network.broadcast_message(&NetMessage::BroadcastAction(payload));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Save state sync — find, zip, and unpack Tiny Glade save "glades".
//
// Tiny Glade does NOT store saves as a single file. Each glade is a folder
// (named by a random UUID) living under:
//   %USERPROFILE%\Saved Games\Tiny Glade\Steam\<SteamID>\saves\<uuid>\
// containing history.json, screenshot.jpg, etc. (see Pounce Light's own
// troubleshooting docs). To sync a glade we zip the whole folder and send
// the zip bytes; the receiving client unzips it into the matching folder.
// ─────────────────────────────────────────────────────────────────────────────

/// Locate the Tiny Glade "saves" directory, e.g.
/// `%USERPROFILE%\Saved Games\Tiny Glade\Steam\<SteamID>\saves`.
/// The SteamID subfolder name is unknown ahead of time, so we pick the most
/// recently modified one under `Steam\`.
pub fn find_save_dir() -> Option<PathBuf> {
    let profile = std::env::var("USERPROFILE").ok()?;
    let saved_games = PathBuf::from(&profile).join("Saved Games");
    let tiny_glade = saved_games.join("Tiny Glade");
    let steam_dir = tiny_glade.join("Steam");

    if steam_dir.is_dir() {
        // Pick the most recently modified SteamID subfolder.
        let mut best: Option<(SystemTime, PathBuf)> = None;
        if let Ok(entries) = std::fs::read_dir(&steam_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let saves = entry.path().join("saves");
                    if saves.is_dir() {
                        let mtime = std::fs::metadata(&saves)
                            .and_then(|m| m.modified())
                            .unwrap_or(SystemTime::UNIX_EPOCH);
                        match &best {
                            None => best = Some((mtime, saves)),
                            Some((existing, _)) if mtime > *existing => best = Some((mtime, saves)),
                            _ => {}
                        }
                    }
                }
            }
        }
        if let Some((_, saves)) = best {
            return Some(saves);
        }
    }

    None
}

/// Returns the most recently modified glade's (folder_name, mtime).
/// "Modified" is determined by the newest mtime of any file inside the
/// glade folder (the folder's own mtime isn't always bumped on Windows
/// when a file inside it changes).
pub fn get_latest_save_info() -> Option<(String, SystemTime, u64)> {
    let save_dir = find_save_dir()?;

    let mut latest: Option<(SystemTime, String, u64)> = None;

    if let Ok(entries) = std::fs::read_dir(&save_dir) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let mut newest_mtime: Option<SystemTime> = None;
            let mut total_size: u64 = 0;
            for f in walkdir::WalkDir::new(entry.path()).into_iter().flatten() {
                if f.file_type().is_file() {
                    if let Ok(meta) = f.metadata() {
                        total_size += meta.len();
                        if let Ok(mtime) = meta.modified() {
                            if newest_mtime.map_or(true, |cur| mtime > cur) {
                                newest_mtime = Some(mtime);
                            }
                        }
                    }
                }
            }
            if let Some(mtime) = newest_mtime {
                match &latest {
                    None => latest = Some((mtime, name, total_size)),
                    Some((existing_mtime, _, _)) if mtime > *existing_mtime => {
                        latest = Some((mtime, name, total_size));
                    }
                    _ => {}
                }
            }
        }
    }

    latest.map(|(mtime, name, size)| (name, mtime, size))
}

/// Zip the most recently modified glade folder in-memory.
/// Returns (glade_uuid_folder_name, zip_bytes).
pub fn read_latest_save() -> Option<(String, Vec<u8>)> {
    let save_dir = find_save_dir()?;
    let (glade_name, _, _) = get_latest_save_info()?;
    let glade_path = save_dir.join(&glade_name);
    let bytes = zip_dir(&glade_path)?;
    Some((glade_name, bytes))
}

/// Zip a directory's contents (recursively) into an in-memory buffer.
/// Paths inside the zip are relative to `dir` (so unzipping recreates the
/// same folder structure without the glade UUID prefix).
fn zip_dir(dir: &std::path::Path) -> Option<Vec<u8>> {
    use std::io::Write;
    use zip::write::FileOptions;

    let mut buf = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut buf);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
        let path = entry.path();
        let rel = path.strip_prefix(dir).ok()?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if entry.file_type().is_dir() {
            writer.add_directory(format!("{}/", rel_str), options).ok()?;
        } else if entry.file_type().is_file() {
            let data = std::fs::read(path).ok()?;
            writer.start_file(rel_str, options).ok()?;
            writer.write_all(&data).ok()?;
        }
    }

    writer.finish().ok()?;
    drop(writer);
    Some(buf.into_inner())
}

/// Unzip received glade bytes into `saves/<glade_name>/`, overwriting any
/// existing files. `glade_name` is the UUID folder name sent by the host.
pub fn write_save_file(glade_name: &str, zip_bytes: &[u8]) -> bool {
    let save_dir = match find_save_dir() {
        Some(d) => d,
        None => {
            debug_log!("[GladeSync ERROR] Cannot write save: save dir not found (is Tiny Glade installed / has it been run at least once?)");
            return false;
        }
    };

    let dest_dir = save_dir.join(glade_name);
    if std::fs::create_dir_all(&dest_dir).is_err() {
        debug_log!("[GladeSync ERROR] Failed to create glade dir: {}", dest_dir.display());
        return false;
    }

    let reader = std::io::Cursor::new(zip_bytes);
    let mut archive = match zip::ZipArchive::new(reader) {
        Ok(a) => a,
        Err(e) => {
            debug_log!("[GladeSync ERROR] Failed to read save zip: {}", e);
            return false;
        }
    };

    for i in 0..archive.len() {
        let mut file = match archive.by_index(i) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let outpath = match file.enclosed_name() {
            Some(p) => dest_dir.join(p),
            None => continue,
        };

        if file.name().ends_with('/') {
            let _ = std::fs::create_dir_all(&outpath);
        } else {
            if let Some(parent) = outpath.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let mut out = match std::fs::File::create(&outpath) {
                Ok(f) => f,
                Err(e) => {
                    debug_log!("[GladeSync ERROR] Failed to write {}: {}", outpath.display(), e);
                    continue;
                }
            };
            if std::io::copy(&mut file, &mut out).is_err() {
                debug_log!("[GladeSync ERROR] Failed to extract {}", outpath.display());
            }
        }
    }

    debug_log!("[GladeSync] Glade written: {}", dest_dir.display());
    true
}
