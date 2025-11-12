#!/usr/bin/env python3
"""
Simple script to examine OpenEXR deep data file headers
without needing to build the full exrs crate
"""

import struct
import sys

def read_string(f):
    """Read null-terminated string"""
    chars = []
    while True:
        c = f.read(1)
        if not c or c == b'\x00':
            break
        chars.append(c)
    return b''.join(chars).decode('ascii')

def read_int(f):
    """Read 32-bit int"""
    return struct.unpack('<i', f.read(4))[0]

def read_uint(f):
    """Read 32-bit unsigned int"""
    return struct.unpack('<I', f.read(4))[0]

def examine_exr(filename):
    print(f"\n{'='*60}")
    print(f"Examining: {filename}")
    print(f"{'='*60}")

    with open(filename, 'rb') as f:
        # Read magic number
        magic = struct.unpack('<I', f.read(4))[0]
        print(f"Magic: 0x{magic:08x}")

        # Read version
        version = struct.unpack('<I', f.read(4))[0]
        file_version = version & 0xFF
        flags = version >> 8

        print(f"File version: {file_version}")
        print(f"Version field: 0x{version:08x}")
        print(f"Flags: 0x{flags:06x}")

        # Flags are in the upper 24 bits, so check bits relative to bit 8
        has_tile = bool(version & 0x00000200)
        has_long_names = bool(version & 0x00000400)
        has_deep = bool(version & 0x00000800)
        has_multipart = bool(version & 0x00001000)

        print(f"  Single tile: {has_tile}")
        print(f"  Long names: {has_long_names}")
        print(f"  Deep data: {has_deep}")
        print(f"  Multi-part: {has_multipart}")

        print("\nHeader attributes:")

        # Read attributes
        while True:
            name = read_string(f)
            if not name:
                break

            attr_type = read_string(f)
            attr_size = read_int(f)
            attr_data = f.read(attr_size)

            print(f"\n  {name} ({attr_type}, {attr_size} bytes)")

            # Parse some common attributes
            if attr_type == 'string':
                value = attr_data[:-1].decode('ascii', errors='replace')
                print(f"    Value: {value}")
            elif attr_type == 'int':
                value = struct.unpack('<i', attr_data)[0]
                print(f"    Value: {value}")
            elif attr_type == 'float':
                value = struct.unpack('<f', attr_data)[0]
                print(f"    Value: {value}")
            elif attr_type == 'box2i':
                xmin, ymin, xmax, ymax = struct.unpack('<iiii', attr_data)
                print(f"    Value: ({xmin}, {ymin}) - ({xmax}, {ymax})")
            elif attr_type == 'compression':
                comp_type = struct.unpack('<B', attr_data)[0]
                comp_names = {
                    0: 'NO_COMPRESSION',
                    1: 'RLE_COMPRESSION',
                    2: 'ZIPS_COMPRESSION',
                    3: 'ZIP_COMPRESSION',
                    4: 'PIZ_COMPRESSION',
                    5: 'PXR24_COMPRESSION',
                    6: 'B44_COMPRESSION',
                    7: 'B44A_COMPRESSION',
                    8: 'DWAA_COMPRESSION',
                    9: 'DWAB_COMPRESSION',
                }
                print(f"    Value: {comp_names.get(comp_type, 'UNKNOWN')}")
            elif attr_type == 'chlist':
                pos = 0
                print("    Channels:")
                while pos < len(attr_data):
                    # Read channel name
                    end = attr_data.find(b'\x00', pos)
                    if end == pos:
                        break
                    ch_name = attr_data[pos:end].decode('ascii')
                    pos = end + 1

                    # Read channel data (16 bytes)
                    pixel_type = struct.unpack('<i', attr_data[pos:pos+4])[0]
                    pLinear = struct.unpack('<?', attr_data[pos+4:pos+5])[0]
                    # 3 bytes reserved
                    xSamp = struct.unpack('<i', attr_data[pos+8:pos+12])[0]
                    ySamp = struct.unpack('<i', attr_data[pos+12:pos+16])[0]
                    pos += 16

                    type_names = {0: 'UINT', 1: 'HALF', 2: 'FLOAT'}
                    print(f"      {ch_name}: {type_names.get(pixel_type, 'UNKNOWN')}, "
                          f"sampling ({xSamp}, {ySamp})")
            elif attr_type == 'lineOrder':
                order = struct.unpack('<B', attr_data)[0]
                order_names = {
                    0: 'INCREASING_Y',
                    1: 'DECREASING_Y',
                    2: 'RANDOM_Y'
                }
                print(f"    Value: {order_names.get(order, 'UNKNOWN')}")
            elif name == 'type':
                value = attr_data[:-1].decode('ascii', errors='replace')
                print(f"    Value: {value}")
                if value in ['deepscanline', 'deeptile']:
                    print("    *** DEEP DATA IMAGE ***")

        print(f"\nFile position after header: {f.tell()}")

        if has_deep:
            print("\n*** This is a DEEP DATA image ***")
        else:
            print("\n*** This is a FLAT image ***")

if __name__ == '__main__':
    files = sys.argv[1:] if len(sys.argv) > 1 else ['Balls.exr', 'Ground.exr']
    for filename in files:
        try:
            examine_exr(filename)
        except Exception as e:
            print(f"Error examining {filename}: {e}")
            import traceback
            traceback.print_exc()
