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

    /// Patch a single function with mov al, 1; ret (B0 01 C3)
    /// Returns true on success, false on failure.
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

    /// Unlock the building boundary limits (is_pos_inside + all is_shape_inside copies)
    /// Rust monomorphization generates multiple copies of is_shape_inside for different
    /// generic type instantiations. We need to patch ALL of them, not just one.
    pub fn unlock_build_borders(&self) -> bool {
        // is_pos_inside: single copy
        let is_pos_inside_rva = 0xAD2950;

        // is_shape_inside: 8 copies found via binary analysis (Rust monomorphization)
        // The game uses different copies for: placement, camera clamping, deletion checks, etc.
        let is_shape_inside_rvas: &[(usize, &str)] = &[
            (0xAD2970,  "is_shape_inside (placement)"),
            (0xE055C0,  "is_shape_inside (camera)"),
            (0x11FA450, "is_shape_inside (copy3)"),
            (0x126A450, "is_shape_inside (copy4)"),
            (0x126A6B0, "is_shape_inside (copy5)"),
            (0x1469E00, "is_shape_inside (copy6)"),
            (0x146A060, "is_shape_inside (copy7)"),
            (0x1CD64A0, "is_shape_inside (copy8)"),
        ];

        unsafe {
            // Patch is_pos_inside
            if !self.patch_function(is_pos_inside_rva, "is_pos_inside") {
                return false;
            }

            // Patch ALL copies of is_shape_inside
            let mut all_ok = true;
            for (rva, name) in is_shape_inside_rvas {
                if !self.patch_function(*rva, name) {
                    all_ok = false;
                }
            }

            if !all_ok {
                eprintln!("[GladeSync] Warning: some is_shape_inside copies failed to patch");
            }
        }

        BORDER_UNLOCKED.store(true, Ordering::SeqCst);
        println!("\x1b[1;32m[GladeSync] ★ Zone de construction DEVERROUILLEE (Mode 4x / Illimite) ★\x1b[0m");
        println!("\x1b[36m[GladeSync] Tu peux desormais construire sans restriction de bordure !\x1b[0m");
        println!("\x1b[36m[GladeSync] Camera et suppression debloquees au-dela des limites !\x1b[0m");
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
