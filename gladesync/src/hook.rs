use crate::network::NetworkManager;
use crate::protocol::{ActionPayload, EditCategory, NetMessage};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::System::Memory::{VirtualAlloc, VirtualProtect, PAGE_EXECUTE_READWRITE};

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

    /// Write an absolute jump (mov rax, addr; jmp rax) at the given address.
    unsafe fn write_jump(from: usize, to: usize) -> bool {
        let mut patch = [0u8; 12];
        patch[0] = 0x48; // REX.W
        patch[1] = 0xB8; // mov rax, imm64
        patch[2..10].copy_from_slice(&(to as u64).to_le_bytes());
        patch[10] = 0xFF; // jmp rax
        patch[11] = 0xE0;
        Self::write_raw(from, &patch)
    }

    /// Allocate executable memory using VirtualAlloc.
    unsafe fn alloc_exec(size: usize) -> usize {
        // MEM_COMMIT = 0x1000, MEM_RESERVE = 0x2000
        let mem = VirtualAlloc(
            std::ptr::null_mut(),
            size,
            0x1000 | 0x2000,
            PAGE_EXECUTE_READWRITE,
        );
        if mem.is_null() {
            0
        } else {
            // Zero the memory
            std::ptr::write_bytes(mem as *mut u8, 0, size);
            mem as usize
        }
    }

    /// Build a code cave that doubles the border struct's float dimensions on first call,
    /// then always returns true (mov al, 1; ret).
    ///
    /// Border struct (rcx):
    ///   [0..4]   = i32 (kind)
    ///   [4..12]  = f64 (half_width)
    ///   [12..20] = f64 (half_height)
    unsafe fn build_border_doubler_cave(flag_ptr: usize) -> usize {
        let cave = Self::alloc_exec(256);
        if cave == 0 {
            return 0;
        }

        let mut code: Vec<u8> = Vec::with_capacity(128);

        // mov r9, flag_ptr          ; 49 B8 <8 bytes>
        code.push(0x49);
        code.push(0xB8);
        code.extend_from_slice(&(flag_ptr as u64).to_le_bytes());

        // cmp byte [r9], 0          ; 41 80 39 00
        code.extend_from_slice(&[0x41, 0x80, 0x39, 0x00]);

        // jne .return_true          ; 75 xx  (short jump)
        code.push(0x75);
        let jne_rel_idx = code.len();
        code.push(0x00); // placeholder

        // mov byte [r9], 1          ; 41 C6 01 01
        code.extend_from_slice(&[0x41, 0xC6, 0x01, 0x01]);

        // --- Double the border dimensions ---
        // movsd xmm0, [rcx+4]      ; F2 0F 10 41 04
        code.extend_from_slice(&[0xF2, 0x0F, 0x10, 0x41, 0x04]);
        // addsd xmm0, xmm0          ; F2 0F 58 C0
        code.extend_from_slice(&[0xF2, 0x0F, 0x58, 0xC0]);
        // movsd [rcx+4], xmm0       ; F2 0F 11 41 04
        code.extend_from_slice(&[0xF2, 0x0F, 0x11, 0x41, 0x04]);
        // movsd xmm0, [rcx+0Ch]     ; F2 0F 10 41 0C
        code.extend_from_slice(&[0xF2, 0x0F, 0x10, 0x41, 0x0C]);
        // addsd xmm0, xmm0          ; F2 0F 58 C0
        code.extend_from_slice(&[0xF2, 0x0F, 0x58, 0xC0]);
        // movsd [rcx+0Ch], xmm0     ; F2 0F 11 41 0C
        code.extend_from_slice(&[0xF2, 0x0F, 0x11, 0x41, 0x0C]);

        // .return_true:
        let return_true_offset = code.len();
        // Fix jne relative offset
        code[jne_rel_idx] = (return_true_offset - (jne_rel_idx + 1)) as u8;

        // mov al, 1                ; B0 01
        code.push(0xB0);
        code.push(0x01);
        // ret                       ; C3
        code.push(0xC3);

        // Copy code to the executable memory
        std::ptr::copy_nonoverlapping(code.as_ptr(), cave as *mut u8, code.len());

        cave
    }

    /// Unlock building borders by doubling the border dimensions (2x zone).
    ///
    /// Installs code caves on is_pos_inside and is_shape_inside that intercept
    /// the first call, double the border struct's float dimensions, then return true.
    /// After 2 seconds, the original function bytes are restored. Now all systems
    /// (placement, camera, deletion) check against the doubled border naturally.
    pub fn unlock_build_borders(&self) -> bool {
        // Static flag bytes (one per function, in case they use different border structs)
        static mut FLAG_POS: u8 = 0;
        static mut FLAG_SHAPE: u8 = 0;

        unsafe {
            let flag_pos = std::ptr::addr_of_mut!(FLAG_POS) as usize;
            let flag_shape = std::ptr::addr_of_mut!(FLAG_SHAPE) as usize;

            // Build code cave for is_pos_inside
            let cave1 = Self::build_border_doubler_cave(flag_pos);
            if cave1 == 0 {
                eprintln!("[GladeSync] Failed to allocate code cave for is_pos_inside");
                return false;
            }

            // Build code cave for is_shape_inside
            let cave2 = Self::build_border_doubler_cave(flag_shape);
            if cave2 == 0 {
                eprintln!("[GladeSync] Failed to allocate code cave for is_shape_inside");
                return false;
            }

            // Install jump from is_pos_inside (RVA 0xAD2950) to cave1
            if !Self::write_jump(self.base_address + 0xAD2950, cave1) {
                eprintln!("[GladeSync] Failed to patch is_pos_inside");
                return false;
            }
            println!("\x1b[32m[GladeSync] Code cave installed on is_pos_inside\x1b[0m");

            // Install jump from is_shape_inside (RVA 0xAD2970) to cave2
            if !Self::write_jump(self.base_address + 0xAD2970, cave2) {
                eprintln!("[GladeSync] Failed to patch is_shape_inside");
                return false;
            }
            println!("\x1b[32m[GladeSync] Code cave installed on is_shape_inside\x1b[0m");

            // Spawn thread to restore original function bytes after 2 seconds.
            // By then, the game has called these functions at least once,
            // so the border struct has been doubled.
            let base = self.base_address;
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(2));

                // Restore is_pos_inside original bytes (21 bytes from scan)
                let pos_original: [u8; 21] = [
                    0x48, 0x89, 0xC8, 0x48, 0x83, 0xC1, 0x04, 0xF6,
                    0x00, 0x01, 0x0F, 0x85, 0xD0, 0x7C, 0x1C, 0x00,
                    0xE9, 0x8B, 0x57, 0x1C, 0x00,
                ];
                if Self::write_raw(base + 0xAD2950, &pos_original) {
                    println!("\x1b[32m[GladeSync] is_pos_inside restored (2x border active)\x1b[0m");
                }

                // Restore is_shape_inside original bytes (12 bytes from scan)
                let shape_original: [u8; 12] = [
                    0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48,
                    0x81, 0xEC, 0xE0, 0x00,
                ];
                if Self::write_raw(base + 0xAD2970, &shape_original) {
                    println!("\x1b[32m[GladeSync] is_shape_inside restored (2x border active)\x1b[0m");
                }
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
