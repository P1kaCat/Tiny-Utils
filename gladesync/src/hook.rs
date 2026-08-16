use crate::network::NetworkManager;
use crate::protocol::{ActionPayload, EditCategory, NetMessage};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::System::Memory::{
    MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_READWRITE, VirtualQuery,
};

pub static BORDER_UNLOCKED: AtomicBool = AtomicBool::new(false);

static CAMERA_POS_ADDR: AtomicUsize = AtomicUsize::new(0);
static CAMERA_ROT_ADDR: AtomicUsize = AtomicUsize::new(0);

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
        self.start_camera_scanner();
        self.start_transform_broadcast();

        let net = Arc::clone(&self.network);
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(500));
                if net.is_active() {}
            }
        });
    }

    pub fn read_camera_data() -> Option<(f32, f32, f32, f32, f32)> {
        let pos_addr = CAMERA_POS_ADDR.load(Ordering::SeqCst);
        if pos_addr == 0 {
            return None;
        }
        unsafe {
            let p = pos_addr as *const f32;
            let x = std::ptr::read_unaligned(p);
            let y = std::ptr::read_unaligned(p.add(1));
            let z = std::ptr::read_unaligned(p.add(2));

            let rot_addr = CAMERA_ROT_ADDR.load(Ordering::SeqCst);
            let (yaw, pitch) = if rot_addr != 0 {
                let r = rot_addr as *const f32;
                (std::ptr::read_unaligned(r), std::ptr::read_unaligned(r.add(1)))
            } else {
                (0.0, 0.0)
            };
            Some((x, y, z, yaw, pitch))
        }
    }

    fn start_transform_broadcast(&self) {
        let net = Arc::clone(&self.network);
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(16));
                if net.is_active() {
                    if let Some((x, y, z, yaw, pitch)) = Self::read_camera_data() {
                        net.update_local_transform(x, y, z, yaw, pitch);
                    }
                    net.maybe_broadcast_transform();
                }
            }
        });
    }

    fn start_camera_scanner(&self) {
        thread::spawn(|| {
            println!("[GladeSync] Camera scanner starting - MOVE YOUR CAMERA AROUND!");

            let regions = Self::scan_writable_regions();
            let total_mb: f64 = regions.iter().map(|(_, s)| *s as f64).sum::<f64>() / 1_048_576.0;
            println!("[GladeSync] Scanning {:.1} MB across {} regions", total_mb, regions.len());

            let snap1 = Self::snapshot_bytes(&regions);
            println!("[GladeSync] Snapshot 1 done. Move your camera!");
            thread::sleep(Duration::from_millis(500));

            let snap2 = Self::snapshot_bytes(&regions);
            println!("[GladeSync] Snapshot 2 done. Keep moving!");
            thread::sleep(Duration::from_millis(500));

            let snap3 = Self::snapshot_bytes(&regions);
            println!("[GladeSync] Snapshot 3 done. Analyzing...");

            let changed_12 = Self::find_changed_triplets(&snap1, &snap2, &regions);
            let changed_23 = Self::find_changed_triplets(&snap2, &snap3, &regions);

            let candidates: Vec<usize> = changed_12.iter()
                .filter(|addr| changed_23.contains(addr))
                .copied()
                .collect();

            println!("[GladeSync] Camera candidates: {}", candidates.len());

            if candidates.is_empty() {
                eprintln!("[GladeSync] No camera found. Make sure to move the camera during the scan!");
                return;
            }

            let addr = candidates[0];
            CAMERA_POS_ADDR.store(addr, Ordering::SeqCst);
            println!("[GladeSync] Camera position found at 0x{:X}", addr);

            unsafe {
                for offset in 3..=16usize {
                    let test_addr = addr + offset * 4;
                    let v1 = std::ptr::read_unaligned(test_addr as *const f32);
                    thread::sleep(Duration::from_millis(100));
                    let v2 = std::ptr::read_unaligned(test_addr as *const f32);
                    if v1.is_finite() && v2.is_finite() && (v1 - v2).abs() > 0.001 {
                        CAMERA_ROT_ADDR.store(test_addr, Ordering::SeqCst);
                        println!("[GladeSync] Camera rotation found at 0x{:X} (+{})", test_addr, offset);
                        break;
                    }
                }
            }

            println!("[GladeSync] Camera tracking active! Player avatars enabled.");
        });
    }

    fn scan_writable_regions() -> Vec<(usize, usize)> {
        let mut regions = Vec::new();
        let mut addr = 1usize;

        loop {
            let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
            let result = unsafe {
                VirtualQuery(addr as *const std::ffi::c_void, &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>())
            };
            if result == 0 { break; }

            if mbi.State == (MEM_COMMIT as u32)
                && (mbi.Protect as u32 & PAGE_READWRITE as u32) != 0
                && mbi.RegionSize <= 16 * 1024 * 1024
            {
                regions.push((mbi.BaseAddress as usize, mbi.RegionSize));
            }

            let next = mbi.BaseAddress as usize + mbi.RegionSize;
            if next <= addr { break; }
            addr = next;
        }

        regions
    }

    fn snapshot_bytes(regions: &[(usize, usize)]) -> Vec<u8> {
        let total: usize = regions.iter().map(|(_, s)| *s).sum();
        let mut buf = vec![0u8; total];

        let mut off = 0usize;
        for (base, size) in regions {
            unsafe {
                std::ptr::copy_nonoverlapping(*base as *const u8, buf.as_mut_ptr().add(off), *size);
            }
            off += size;
        }
        buf
    }

    fn find_changed_triplets(snap_a: &[u8], snap_b: &[u8], regions: &[(usize, usize)]) -> Vec<usize> {
        let mut result = Vec::new();
        let mut off = 0usize;

        for (base, size) in regions {
            let region_a = &snap_a[off..off + size];
            let region_b = &snap_b[off..off + size];

            let mut i = 0usize;
            while i + 12 <= *size {
                let a_chunk = &region_a[i..i + 12];
                let b_chunk = &region_b[i..i + 12];

                let mut all_changed = true;
                for j in 0..12 {
                    if a_chunk[j] == b_chunk[j] {
                        all_changed = false;
                        break;
                    }
                }

                if all_changed {
                    let f1a = f32::from_le_bytes([a_chunk[0], a_chunk[1], a_chunk[2], a_chunk[3]]);
                    let f2a = f32::from_le_bytes([a_chunk[4], a_chunk[5], a_chunk[6], a_chunk[7]]);
                    let f3a = f32::from_le_bytes([a_chunk[8], a_chunk[9], a_chunk[10], a_chunk[11]]);
                    let f1b = f32::from_le_bytes([b_chunk[0], b_chunk[1], b_chunk[2], b_chunk[3]]);
                    let f2b = f32::from_le_bytes([b_chunk[4], b_chunk[5], b_chunk[6], b_chunk[7]]);
                    let f3b = f32::from_le_bytes([b_chunk[8], b_chunk[9], b_chunk[10], b_chunk[11]]);

                    if f1a.is_finite() && f2a.is_finite() && f3a.is_finite()
                        && f1b.is_finite() && f2b.is_finite() && f3b.is_finite()
                        && f1a.abs() < 100000.0 && f2a.abs() < 100000.0 && f3a.abs() < 100000.0
                        && f1b.abs() < 100000.0 && f2b.abs() < 100000.0 && f3b.abs() < 100000.0
                        && (f1a - f1b).abs() > 0.01
                        && (f2a - f2b).abs() > 0.01
                        && (f3a - f3b).abs() > 0.01
                    {
                        result.push(base + i);
                        i += 12;
                        continue;
                    }
                }
                i += 4;
            }
            off += size;
        }
        result
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
