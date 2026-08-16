pub mod console;
pub mod hook;
pub mod network;
pub mod protocol;
pub mod proxy;
pub mod ui;

use std::ffi::c_void;
use std::sync::Arc;
use std::thread;
use windows_sys::Win32::Foundation::{BOOL, HMODULE, TRUE};
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllMain(
    _hinst_dll: HMODULE,
    fdw_reason: u32,
    _lpv_reserved: *mut c_void,
) -> BOOL {
    match fdw_reason {
        DLL_PROCESS_ATTACH => {
            proxy::init_proxy();
            thread::spawn(move || {
                let network = network::NetworkManager::new();
                ui::start_ui_thread(Arc::clone(&network));
                let hook_engine = hook::HookEngine::new(Arc::clone(&network));
                hook_engine.start();
            });
        }
        DLL_PROCESS_DETACH => {}
        _ => {}
    }
    TRUE
}
