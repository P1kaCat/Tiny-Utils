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

        // Automatically unlock building borders (4x / Infinite zone)
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

    /// Unlock the building boundary limits (GladeBorder is_pos_inside & is_shape_inside)
    pub fn unlock_build_borders(&self) -> bool {
        // Offsets verified via PDB symbols:
        // is_pos_inside:  RVA 0xAD2950
        // is_shape_inside: RVA 0xAD2970
        let is_pos_inside_addr = (self.base_address + 0xAD2950) as *mut u8;
        let is_shape_inside_addr = (self.base_address + 0xAD2970) as *mut u8;

        unsafe {
            // Opcode: mov al, 1; ret -> B0 01 C3
            let patch_bytes = [0xB0, 0x01, 0xC3];

            let mut old_protect = 0u32;
            if VirtualProtect(is_pos_inside_addr as _, 32, PAGE_EXECUTE_READWRITE, &mut old_protect) != 0 {
                std::ptr::copy_nonoverlapping(patch_bytes.as_ptr(), is_pos_inside_addr, patch_bytes.len());
                VirtualProtect(is_pos_inside_addr as _, 32, old_protect, &mut old_protect);
            } else {
                eprintln!("[GladeSync] Failed to patch is_pos_inside memory protection");
                return false;
            }

            if VirtualProtect(is_shape_inside_addr as _, 32, PAGE_EXECUTE_READWRITE, &mut old_protect) != 0 {
                std::ptr::copy_nonoverlapping(patch_bytes.as_ptr(), is_shape_inside_addr, patch_bytes.len());
                VirtualProtect(is_shape_inside_addr as _, 32, old_protect, &mut old_protect);
            } else {
                eprintln!("[GladeSync] Failed to patch is_shape_inside memory protection");
                return false;
            }
        }

        BORDER_UNLOCKED.store(true, Ordering::SeqCst);
        println!("\x1b[1;32m[GladeSync] ★ Zone de construction DEVERROUILLEE (Mode 4x / Illimite) ★\x1b[0m");
        println!("\x1b[36m[GladeSync] Tu peux desormais construire sans restriction de bordure !\x1b[0m");
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
