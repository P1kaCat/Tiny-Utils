use crate::network::NetworkManager;
use crate::protocol::{ActionPayload, EditCategory, NetMessage};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE};

pub static BORDER_UNLOCKED: AtomicBool = AtomicBool::new(false);

// Captured border struct pointer (written by the patched is_pos_inside, read from Rust)
static BORDER_PTR: AtomicUsize = AtomicUsize::new(0);

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

    /// Write bytes to an address, changing memory protection temporarily.
    unsafe fn write_raw(addr: usize, bytes: &[u8]) -> bool {
        let ptr = addr as *mut u8;
        let mut old = 0u32;
        if VirtualProtect(ptr as _, bytes.len(), PAGE_EXECUTE_READWRITE, &mut old) != 0 {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            VirtualProtect(ptr as _, bytes.len(), old, &mut old);
            true
        } else {
            false
        }
    }

    /// Unlock building borders by doubling the border dimensions (2x zone).
    ///
    /// Step 1: Patch is_pos_inside to capture the border struct pointer (rcx) into
    ///         a global static, then return true (temporary, unlimited placement).
    /// Step 2: A Rust thread polls for the captured pointer (up to 5 seconds).
    /// Step 3: Once captured, the thread doubles the f64 dimensions at [ptr+4]
    ///         and [ptr+12] (half_width * 2, half_height * 2).
    /// Step 4: The thread restores the original bytes of is_pos_inside and
    ///         is_shape_inside. Now all systems check against the 2x border.
    pub fn unlock_build_borders(&self) -> bool {
        unsafe {
            let border_ptr_addr = BORDER_PTR.as_ptr() as usize;

            // --- Patch is_pos_inside to capture border struct pointer ---
            // mov rax, <border_ptr_addr>   ; 48 B8 <8 bytes>
            // mov [rax], rcx                ; 48 89 08
            // mov al, 1                     ; B0 01
            // ret                           ; C3
            let mut patch: Vec<u8> = Vec::with_capacity(16);
            patch.extend_from_slice(&[0x48, 0xB8]);
            patch.extend_from_slice(&(border_ptr_addr as u64).to_le_bytes());
            patch.extend_from_slice(&[0x48, 0x89, 0x08]);
            patch.extend_from_slice(&[0xB0, 0x01]);
            patch.push(0xC3);

            if !Self::write_raw(self.base_address + 0xAD2950, &patch) {
                eprintln!("[GladeSync] Failed to patch is_pos_inside");
                return false;
            }
            println!("\x1b[32m[GladeSync] is_pos_inside patched (capturing border ptr)\x1b[0m");

            // --- Patch is_shape_inside with mov al,1; ret (temporary) ---
            if !Self::write_raw(self.base_address + 0xAD2970, &[0xB0, 0x01, 0xC3]) {
                eprintln!("[GladeSync] Failed to patch is_shape_inside");
                return false;
            }
            println!("\x1b[32m[GladeSync] is_shape_inside patched (temporary)\x1b[0m");

            // --- Spawn thread to capture pointer, double border, restore functions ---
            let base = self.base_address;
            thread::spawn(move || {
                // Wait for is_pos_inside to be called (border pointer captured)
                let mut captured = false;
                for i in 0..100 {
                    thread::sleep(Duration::from_millis(50));
                    let ptr = BORDER_PTR.load(Ordering::SeqCst);
                    if ptr != 0 {
                        println!("\x1b[32m[GladeSync] Border struct captured at 0x{:X}\x1b[0m", ptr);
                        captured = true;
                        break;
                    }
                    if i == 20 {
                        println!("\x1b[33m[GladeSync] Waiting for border struct... (game not calling is_pos_inside yet)\x1b[0m");
                    }
                }

                if !captured {
                    eprintln!("\x1b[31m[GladeSync] Border struct pointer not captured after 5s\x1b[0m");
                    eprintln!("\x1b[31m[GladeSync] Falling back to unlimited mode\x1b[0m");
                    return;
                }

                let ptr = BORDER_PTR.load(Ordering::SeqCst) as *mut u8;

                // Double the border dimensions
                // [ptr+4] = f64 half_width, [ptr+12] = f64 half_height
                let mut old = 0u32;
                if VirtualProtect(ptr as _, 32, PAGE_EXECUTE_READWRITE, &mut old) != 0 {
                    let w_ptr = ptr.add(4).cast::<f64>();
                    let h_ptr = ptr.add(12).cast::<f64>();
                    let w_val = w_ptr.read_unaligned();
                    let h_val = h_ptr.read_unaligned();

                    if w_val > 0.0 && h_val > 0.0 && w_val < 100000.0 && h_val < 100000.0 {
                        w_ptr.write_unaligned(w_val * 2.0);
                        h_ptr.write_unaligned(h_val * 2.0);
                        println!(
                            "\x1b[32m[GladeSync] Border DOUBLED: {:.2} x {:.2} -> {:.2} x {:.2}\x1b[0m",
                            w_val, h_val, w_val * 2.0, h_val * 2.0
                        );
                    } else {
                        eprintln!(
                            "\x1b[33m[GladeSync] Border values look wrong: w={:.2} h={:.2}, skipping\x1b[0m",
                            w_val, h_val
                        );
                    }
                    VirtualProtect(ptr as _, 32, old, &mut old);
                } else {
                    eprintln!("\x1b[31m[GladeSync] VirtualProtect failed on border struct\x1b[0m");
                }

                // Small delay to ensure the doubled values are visible
                thread::sleep(Duration::from_millis(100));

                // Restore is_pos_inside original bytes (21 bytes from binary scan)
                let pos_original: [u8; 21] = [
                    0x48, 0x89, 0xC8, 0x48, 0x83, 0xC1, 0x04, 0xF6,
                    0x00, 0x01, 0x0F, 0x85, 0xD0, 0x7C, 0x1C, 0x00,
                    0xE9, 0x8B, 0x57, 0x1C, 0x00,
                ];
                if Self::write_raw(base + 0xAD2950, &pos_original) {
                    println!("\x1b[32m[GladeSync] is_pos_inside restored (2x border active)\x1b[0m");
                }

                // Restore is_shape_inside original bytes (12 bytes from binary scan)
                let shape_original: [u8; 12] = [
                    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48,
                    0x81, 0xEC, 0xE0, 0x00,
                ];
                if Self::write_raw(base + 0xAD2970, &shape_original) {
                    println!("\x1b[32m[GladeSync] is_shape_inside restored (2x border active)\x1b[0m");
                }

                println!("\x1b[1;32m[GladeSync] ★ Zone 2x active ★\x1b[0m");
            });
        }

        BORDER_UNLOCKED.store(true, Ordering::SeqCst);
        println!("\x1b[1;32m[GladeSync] ★ Zone de construction x2 (agrandie) ★\x1b[0m");
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
