pub mod console;
pub mod hook;
pub mod network;
pub mod protocol;
pub mod proxy;
pub mod ui;

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use windows_sys::Win32::Foundation::{BOOL, HMODULE, TRUE};
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

// Guards against double-initialization. Some proxy-DLL loading paths can
// trigger DLL_PROCESS_ATTACH more than once in the same process (e.g. the
// game resolving the same physical DLL via two different search paths).
// Without this guard, a second attach would spawn a second network stack,
// UI thread, and hook loop — causing duplicate debug consoles, doubled
// hosting attempts, etc.
static INITIALIZED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllMain(
    _hinst_dll: HMODULE,
    fdw_reason: u32,
    _lpv_reserved: *mut c_void,
) -> BOOL {
    match fdw_reason {
        DLL_PROCESS_ATTACH => {
            if INITIALIZED.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                proxy::init_proxy();
                thread::spawn(move || {
                    let network = network::NetworkManager::new();
                    ui::start_ui_thread(Arc::clone(&network));
                    let hook_engine = hook::HookEngine::new(Arc::clone(&network));
                    hook_engine.start();
                });
            }
        }
        DLL_PROCESS_DETACH => {}
        _ => {}
    }
    TRUE
}
