"""
Scan Tiny Glade executable to find border/camera/deletion boundary functions.
Run: python scan_glade.py "C:\path\to\tiny-glade.exe"
"""

import sys
import struct
import os

def parse_pe(data):
    """Parse PE headers to find sections and image base."""
    if data[:2] != b'MZ':
        print("Not a valid PE file!")
        return None
    
    pe_offset = struct.unpack_from('<I', data, 0x3C)[0]
    if data[pe_offset:pe_offset+4] != b'PE\x00\x00':
        print("Invalid PE signature!")
        return None
    
    num_sections = struct.unpack_from('<H', data, pe_offset + 6)[0]
    optional_header_size = struct.unpack_from('<H', data, pe_offset + 20)[0]
    
    optional_offset = pe_offset + 24
    magic = struct.unpack_from('<H', data, optional_offset)[0]
    if magic == 0x20b:  # PE32+
        image_base = struct.unpack_from('<Q', data, optional_offset + 24)[0]
    else:  # PE32
        image_base = struct.unpack_from('<I', data, optional_offset + 28)[0]
    
    section_offset = optional_offset + optional_header_size
    sections = []
    for i in range(num_sections):
        offset = section_offset + i * 40
        name = data[offset:offset+8].rstrip(b'\x00').decode('ascii', errors='replace')
        virtual_size = struct.unpack_from('<I', data, offset + 8)[0]
        virtual_addr = struct.unpack_from('<I', data, offset + 12)[0]
        raw_size = struct.unpack_from('<I', data, offset + 16)[0]
        raw_offset = struct.unpack_from('<I', data, offset + 20)[0]
        sections.append({
            'name': name,
            'virtual_addr': virtual_addr,
            'virtual_size': virtual_size,
            'raw_offset': raw_offset,
            'raw_size': raw_size,
        })
        print(f"  Section: {name:8s}  VA=0x{virtual_addr:08X}  Size=0x{raw_size:08X}  Raw=0x{raw_offset:08X}")
    
    return image_base, sections

def rva_to_offset(rva, sections):
    for s in sections:
        if s['virtual_addr'] <= rva < s['virtual_addr'] + max(s['virtual_size'], s['raw_size']):
            return rva - s['virtual_addr'] + s['raw_offset']
    return None

def offset_to_rva(offset, sections):
    for s in sections:
        if s['raw_offset'] <= offset < s['raw_offset'] + s['raw_size']:
            return offset - s['raw_offset'] + s['virtual_addr']
    return None

def dump_bytes(data, offset, length=64, label=""):
    if label:
        print(f"\n--- {label} ---")
    for i in range(0, length, 16):
        if offset + i >= len(data):
            break
        chunk = data[offset+i:offset+i+16]
        hex_str = ' '.join(f'{b:02X}' for b in chunk)
        ascii_str = ''.join(chr(b) if 32 <= b < 127 else '.' for b in chunk)
        print(f"  {offset+i:08X}  {hex_str:<48s}  {ascii_str}")

def find_function_start(data, offset, max_back=256):
    for i in range(max_back):
        pos = offset - i
        if pos < 0:
            return None
        if data[pos] == 0xC3 and (pos + 1 < len(data)):
            next_byte = data[pos + 1]
            if next_byte in (0xCC, 0x90, 0x55, 0x48, 0x40, 0x53, 0x56, 0x57):
                func_start = pos + 1
                while func_start < len(data) and data[func_start] in (0xCC, 0x90):
                    func_start += 1
                return func_start
    return None

def find_function_end(data, offset, max_forward=512):
    for i in range(max_forward):
        pos = offset + i
        if pos >= len(data):
            return None
        if data[pos] == 0xC3:
            return pos
    return None

def search_pattern(data, pattern, start=0):
    results = []
    pos = start
    while True:
        pos = data.find(pattern, pos)
        if pos == -1:
            break
        results.append(pos)
        pos += 1
    return results

def search_strings(data, sections, keywords):
    results = []
    text = data.decode('latin-1', errors='replace')
    for kw in keywords:
        lower_kw = kw.lower()
        pos = 0
        while True:
            pos = text.lower().find(lower_kw, pos)
            if pos == -1:
                break
            start = pos
            while start > 0 and ord(text[start-1]) >= 32 and ord(text[start-1]) < 127:
                start -= 1
            end = pos + len(kw)
            while end < len(text) and ord(text[end]) >= 32 and ord(text[end]) < 127:
                end += 1
            s = text[start:end].strip()
            if len(s) > 3 and len(s) < 200:
                rva = offset_to_rva(pos, sections)
                results.append((rva, pos, s))
            pos += 1
    return results

def main():
    if len(sys.argv) < 2:
        print("Usage: python scan_glade.py <tiny-glade.exe>")
        print('Example: python scan_glade.py "C:\\Users\\kylli\\Downloads\\STG Games\\Tiny Glade\\tiny-glade.exe"')
        input("Press Enter to exit...")
        sys.exit(1)
    
    exe_path = sys.argv[1]
    if not os.path.exists(exe_path):
        print(f"File not found: {exe_path}")
        input("Press Enter to exit...")
        sys.exit(1)
    
    print(f"Reading: {exe_path}")
    print(f"File size: {os.path.getsize(exe_path) / 1024 / 1024:.1f} MB")
    
    with open(exe_path, 'rb') as f:
        data = f.read()
    
    print(f"\n=== PE SECTIONS ===")
    result = parse_pe(data)
    if not result:
        input("Press Enter to exit...")
        sys.exit(1)
    
    image_base, sections = result
    print(f"Image Base: 0x{image_base:016X}")
    
    KNOWN_OFFSETS = {
        'is_pos_inside':  0xAD2950,
        'is_shape_inside': 0xAD2970,
    }
    
    print(f"\n=== KNOWN FUNCTION BYTES (before patching) ===")
    known_patterns = []
    for name, rva in KNOWN_OFFSETS.items():
        offset = rva_to_offset(rva, sections)
        if offset is None:
            print(f"\n{name} (RVA 0x{rva:08X}): CANNOT MAP TO FILE OFFSET")
            continue
        print(f"\n{name} (RVA 0x{rva:08X}, file offset 0x{offset:08X}):")
        dump_bytes(data, offset, 96, name)
        pattern = data[offset:offset+16]
        known_patterns.append((name, rva, pattern))
    
    print(f"\n\n=== SEARCHING FOR DUPLICATE COPIES OF KNOWN FUNCTIONS ===")
    for name, original_rva, pattern in known_patterns:
        print(f"\nSearching for copies of {name} (pattern: {' '.join(f'{b:02X}' for b in pattern[:16])})")
        matches = search_pattern(data, pattern)
        for m in matches:
            rva = offset_to_rva(m, sections)
            if rva and rva != original_rva:
                print(f"  FOUND COPY at RVA 0x{rva:08X} (file offset 0x{m:08X})")
                dump_bytes(data, m, 64, f"Copy of {name}")
    
    print(f"\n\n=== FUNCTIONS NEAR KNOWN OFFSETS (+-0x2000) ===")
    for name, rva in KNOWN_OFFSETS.items():
        offset = rva_to_offset(rva, sections)
        if offset is None:
            continue
        print(f"\n--- Near {name} (RVA 0x{rva:08X}) ---")
        found = []
        scan_range = 0x2000
        for delta in range(-scan_range, scan_range, 1):
            pos = offset + delta
            if pos < 0 or pos >= len(data) - 4:
                continue
            if data[pos] == 0xC3 and pos + 1 < len(data):
                next_b = data[pos + 1]
                if next_b in (0xCC, 0x90):
                    func_start = pos + 1
                    while func_start < len(data) and data[func_start] in (0xCC, 0x90):
                        func_start += 1
                    if func_start < len(data):
                        func_rva = offset_to_rva(func_start, sections)
                        if func_rva and abs(func_rva - rva) < scan_range:
                            func_end = find_function_end(data, func_start, 256)
                            if func_end:
                                func_size = func_end - func_start + 1
                                if 10 < func_size < 500:
                                    found.append((func_rva, func_start, func_size))
        seen = set()
        for func_rva, func_start, func_size in found:
            if func_start in seen:
                continue
            seen.add(func_start)
            end_bytes = data[func_start+func_size-4:func_start+func_size]
            end_hex = ' '.join(f'{b:02X}' for b in end_bytes)
            print(f"  RVA 0x{func_rva:08X}  size={func_size:4d}  end: {end_hex}")
    
    print(f"\n\n=== SEARCHING FOR BORDER/CAMERA/BOUNDARY STRINGS ===")
    keywords = [
        'border', 'boundary', 'bounds', 'limit', 'clamp',
        'camera', 'is_inside', 'is_within', 'contains',
        'glade', 'build_area', 'play_area', 'world_bound',
        'delete', 'remove', 'erase', 'destroy',
        'pos_inside', 'shape_inside', 'position_inside',
        'extent', 'clamp_pos', 'clamp_camera',
        'out_of_bound', 'outside', 'in_range',
    ]
    string_results = search_strings(data, sections, keywords)
    if string_results:
        seen = set()
        for rva, offset, s in string_results:
            if s in seen:
                continue
            seen.add(s)
            print(f"  RVA 0x{(rva or 0):08X}  offset 0x{offset:08X}  \"{s}\"")
    else:
        print("  No matching strings found.")
    
    print(f"\n\n=== FUNCTIONS THAT RETURN TRUE (mov al,1; ret = B0 01 C3) ===")
    true_pattern = bytes([0xB0, 0x01, 0xC3])
    true_matches = search_pattern(data, true_pattern)
    for m in true_matches[:50]:
        rva = offset_to_rva(m, sections)
        if rva:
            func_start = find_function_start(data, m, 128)
            if func_start:
                func_rva = offset_to_rva(func_start, sections)
                func_size = m - func_start + 3
                if 5 < func_size < 200:
                    print(f"  RVA 0x{rva:08X}, func_start RVA 0x{func_rva:08X}, size={func_size}")
    
    print(f"\n\n=== FUNCTIONS THAT RETURN FALSE (mov al,0; ret = B0 00 C3) ===")
    false_pattern = bytes([0xB0, 0x00, 0xC3])
    false_matches = search_pattern(data, false_pattern)
    for m in false_matches[:50]:
        rva = offset_to_rva(m, sections)
        if rva:
            func_start = find_function_start(data, m, 128)
            if func_start:
                func_rva = offset_to_rva(func_start, sections)
                func_size = m - func_start + 3
                if 5 < func_size < 200:
                    print(f"  RVA 0x{rva:08X}, func_start RVA 0x{func_rva:08X}, size={func_size}")
                    dump_bytes(data, func_start, min(func_size + 8, 64), f"Bool func at 0x{func_rva:08X}")
    
    print(f"\n\n=== DONE ===")
    print("Copy all the output above and send it to me (Kaelo).")
    input("Press Enter to exit...")

if __name__ == '__main__':
    main()
