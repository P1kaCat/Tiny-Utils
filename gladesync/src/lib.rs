pub mod console;
pub mod hook;
pub mod network;
pub mod protocol;
pub mod proxy;

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
            // Forward proxy to system DXGI first
            proxy::init_proxy();

            // Spawn initialization in a detached thread to prevent blocking DllMain
            thread::spawn(move || {
                let network = network::NetworkManager::new();
                
                // Spawn the interactive in-game console
                console::start_console_thread(Arc::clone(&network));

                // Initialize the hook engine
                let hook_engine = hook::HookEngine::new(Arc::clone(&network));
                hook_engine.start();
            });
        }
        DLL_PROCESS_DETACH => {
            // Cleanup logic if needed
        }
        _ => {}
    }
    TRUE
}
