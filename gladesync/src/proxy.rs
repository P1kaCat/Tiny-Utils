use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};
use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

static ORIGINAL_DXGI: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

pub fn init_proxy() -> bool {
    // Load the genuine system dxgi.dll from System32
    let system_path = "C:\\Windows\\System32\\dxgi.dll\0";
    let wide_path: Vec<u16> = system_path.encode_utf16().collect();
    
    let handle = unsafe { LoadLibraryW(wide_path.as_ptr()) };
    if handle.is_null() {
        eprintln!("[GladeSync] Failed to load original System32 dxgi.dll");
        return false;
    }
    
    ORIGINAL_DXGI.store(handle as *mut c_void, Ordering::SeqCst);
    println!("[GladeSync] Loaded system dxgi.dll successfully at {:p}", handle);
    true
}

fn get_original_proc(name: &[u8]) -> *const c_void {
    let handle = ORIGINAL_DXGI.load(Ordering::SeqCst) as HMODULE;
    if handle.is_null() {
        return std::ptr::null();
    }
    unsafe {
        match GetProcAddress(handle, name.as_ptr()) {
            Some(proc) => proc as *const c_void,
            None => std::ptr::null(),
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn CreateDXGIFactory(riid: *const c_void, pp_factory: *mut *mut c_void) -> i32 {
    let proc_addr = get_original_proc(b"CreateDXGIFactory\0");
    if proc_addr.is_null() {
        return -1;
    }
    let func: unsafe extern "system" fn(*const c_void, *mut *mut c_void) -> i32 = std::mem::transmute(proc_addr);
    func(riid, pp_factory)
}

#[no_mangle]
pub unsafe extern "system" fn CreateDXGIFactory1(riid: *const c_void, pp_factory: *mut *mut c_void) -> i32 {
    let proc_addr = get_original_proc(b"CreateDXGIFactory1\0");
    if proc_addr.is_null() {
        return -1;
    }
    let func: unsafe extern "system" fn(*const c_void, *mut *mut c_void) -> i32 = std::mem::transmute(proc_addr);
    func(riid, pp_factory)
}

#[no_mangle]
pub unsafe extern "system" fn CreateDXGIFactory2(flags: u32, riid: *const c_void, pp_factory: *mut *mut c_void) -> i32 {
    let proc_addr = get_original_proc(b"CreateDXGIFactory2\0");
    if proc_addr.is_null() {
        return -1;
    }
    let func: unsafe extern "system" fn(u32, *const c_void, *mut *mut c_void) -> i32 = std::mem::transmute(proc_addr);
    func(flags, riid, pp_factory)
}

#[no_mangle]
pub unsafe extern "system" fn DXGIGetDebugInterface1(flags: u32, riid: *const c_void, pp_debug: *mut *mut c_void) -> i32 {
    let proc_addr = get_original_proc(b"DXGIGetDebugInterface1\0");
    if proc_addr.is_null() {
        return -1;
    }
    let func: unsafe extern "system" fn(u32, *const c_void, *mut *mut c_void) -> i32 = std::mem::transmute(proc_addr);
    func(flags, riid, pp_debug)
}
