use crate::network::NetworkManager;
use crate::protocol::{ActionPayload, EditCategory, NetMessage};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE};

pub static BORDER_UNLOCKED: AtomicBool = AtomicBool::new(false);

pub struct HookEngine {
    base_address: usize,
    network: Arc<NetworkManager>,
}

impl HookEngine {
    pub fn new(network: Arc<NetworkManager>) -> Self {
        let base_address = unsafe { GetModuleHandleA(std::ptr::null()) as usize };
        Self {
            base_address,
            network,
        }
    }

    pub fn start(&self) {
        println!("\x1b[32m[GladeSync Engine] Game Base Address: 0x{:X}\x1b[0m", self.base_address);
        self.unlock_build_borders();

        let net = Arc::clone(&self.network);
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(500));
                if net.is_active() {
                    // Periodic maintenance
                }
            }
        });
    }

    /// Patch a single function with mov al, 1; ret (B0 01 C3)
    unsafe fn patch_function(&self, rva: usize, name: &str) -> bool {
        let addr = (self.base_address + rva) as *mut u8;
        let patch_bytes = [0xB0u8, 0x01, 0xC3]; // mov al, 1; ret

        let mut old_protect = 0u32;
        if VirtualProtect(addr as _, 32, PAGE_EXECUTE_READWRITE, &mut old_protect) != 0 {
            std::ptr::copy_nonoverlapping(patch_bytes.as_ptr(), addr, patch_bytes.len());
            VirtualProtect(addr as _, 32, old_protect, &mut old_protect);
            println!("\x1b[32m[GladeSync] Patched {} at RVA 0x{:08X}\x1b[0m", name, rva);
            true
        } else {
            eprintln!("\x1b[31m[GladeSync] Failed to patch {} at RVA 0x{:08X}\x1b[0m", name, rva);
            false
        }
    }

    /// Unlock the building boundary limits
    pub fn unlock_build_borders(&self) -> bool {
        unsafe {
            self.patch_function(0xAD2950, "is_pos_inside");
            self.patch_function(0xAD2970, "is_shape_inside");
        }

        BORDER_UNLOCKED.store(true, Ordering::SeqCst);
        println!("\x1b[1;32m[GladeSync] ★ Zone de construction DEVERROUILLEE ★\x1b[0m");
        true
    }

    pub fn on_local_edit(&self, category: EditCategory, hex_data: String) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let payload = ActionPayload {
            edit_category: category,
            action_id: now,
            timestamp: now,
            data_hex: hex_data,
        };

        self.network.broadcast_message(&NetMessage::BroadcastAction(payload));
    }
}
