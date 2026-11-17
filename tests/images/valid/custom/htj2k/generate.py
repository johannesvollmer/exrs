#!/usr/bin/env python3
# Regenerates the HTJ2K fixtures in this directory, which tests/htj2k.rs
# reads. Needs `exrmetrics` from OpenEXR 3.4 or later (3.5.3 was used);
# older versions write ZIP when asked for HTJ2K. Run it from this directory:
#
#     python3 generate.py
#
# For each case it writes `<name>.exr` uncompressed, straight from Python,
# then asks OpenEXR to convert it to `<name>_htj2k32.exr` and
# `<name>_htj2k256.exr`. The test decodes the HTJ2K files with exrs and
# compares every sample bit for bit with the uncompressed file. HTJ2K in
# OpenEXR is lossless, so they must match exactly.
#
# The samples are smooth ramps, so that HTJ2K makes every chunk smaller and
# OpenEXR stores none of them raw (raw chunks would bypass the decoder;
# this script checks that every chunk carries an HTJ2K header). Special
# values are sprinkled through the ramps: NaNs with payloads, infinities,
# negative zero, denormals, the largest finite values and extreme UINTs.
#
# The cases cover what OpenEXR's encoder does differently by layout:
#   rgba_half       R, G, B half with A: the colour transform on 16-bit
#                   components; the file orders channels A, B, G, R and the
#                   codestream R, G, B, A. The data window starts at negative
#                   coordinates.
#   rgb_float_uint  R, G, B float with a float Z and a UINT id: the colour
#                   transform on 32-bit components, which the decoder runs
#                   in 64-bit arithmetic. The last chunk of each is short.
#   mixed_rgb       R half, G float, B half: no colour transform, since the
#                   types differ, and components of different depths.
#   y_half          a single channel, no colour transform.
#   tiled           a tiled file, 16x16 tiles over 45x44 pixels, so the
#                   right and bottom tiles are partial.
import os
import struct
import subprocess

HALF, FLOAT, UINT = 1, 2, 0

HALF_SPECIALS = [0x7C00, 0xFC00, 0x7E01, 0xFD23, 0x8000, 0x0001, 0x83FF, 0x7BFF, 0xFBFF, 0x0000]
FLOAT_SPECIALS = [
    0x7F800000, 0xFF800000, 0x7FC00001, 0xFFA5A5A5, 0x80000000,
    0x00000001, 0x807FFFFF, 0x7F7FFFFF, 0xFF7FFFFF, 0x00000000,
]
UINT_SPECIALS = [0xFFFFFFFF, 0x80000000, 0x7FFFFFFF, 0x00000000, 0x00000001]


def sample_bytes(kind, channel_index, x, y):
    """One sample, little-endian: a ramp, or now and then a special value."""
    special = (x * 7 + y * 13 + channel_index * 5) % 193 == 0
    pick = x + y + channel_index
    if kind == HALF:
        if special:
            return struct.pack("<H", HALF_SPECIALS[pick % len(HALF_SPECIALS)])
        value = 0.25 + 0.031 * x - 0.017 * y + 0.5 * channel_index
        return struct.pack("<e", value)
    if kind == FLOAT:
        if special:
            return struct.pack("<I", FLOAT_SPECIALS[pick % len(FLOAT_SPECIALS)])
        # HTJ2K codes the bits of a float as an integer, so a ramp of a few
        # ULPs per pixel compresses, where one spanning many exponents
        # would not
        sign = -1.0 if channel_index % 2 else 1.0
        value = sign * (100.0 + x * 2.0**-14 + y * 2.0**-12)
        return struct.pack("<f", value)
    if special:
        return struct.pack("<I", UINT_SPECIALS[pick % len(UINT_SPECIALS)])
    return struct.pack("<I", 1000 * x + y + 7 * channel_index)


def attribute(name, kind, value):
    return name.encode() + b"\0" + kind.encode() + b"\0" + struct.pack("<i", len(value)) + value


def header(channels, x0, y0, width, height, tile_size):
    chlist = b"".join(
        name.encode() + b"\0" + struct.pack("<iB3xii", kind, 0, 1, 1)
        for name, kind in channels
    ) + b"\0"
    window = struct.pack("<iiii", x0, y0, x0 + width - 1, y0 + height - 1)
    attributes = [
        attribute("channels", "chlist", chlist),
        attribute("compression", "compression", b"\0"),
        attribute("dataWindow", "box2i", window),
        attribute("displayWindow", "box2i", window),
        attribute("lineOrder", "lineOrder", b"\0"),
        attribute("pixelAspectRatio", "float", struct.pack("<f", 1.0)),
        attribute("screenWindowCenter", "v2f", struct.pack("<ff", 0.0, 0.0)),
        attribute("screenWindowWidth", "float", struct.pack("<f", 1.0)),
    ]
    flags = 2
    if tile_size:
        attributes.append(attribute("tiles", "tiledesc", struct.pack("<IIB", tile_size, tile_size, 0)))
        flags |= 0x200
    return b"\x76\x2f\x31\x01" + struct.pack("<I", flags) + b"".join(attributes) + b"\0"


def pixel_rows(channels, x0, y0, x_range, y_range):
    """The uncompressed data of a block: line by line, channels in order."""
    data = bytearray()
    for y in y_range:
        for index, (_, kind) in enumerate(channels):
            for x in x_range:
                data += sample_bytes(kind, index, x - x0, y - y0)
    return bytes(data)


def write_uncompressed(path, channels, x0, y0, width, height, tile_size=None):
    channels = sorted(channels)
    head = header(channels, x0, y0, width, height, tile_size)
    chunks = []
    if tile_size:
        for ty in range((height + tile_size - 1) // tile_size):
            for tx in range((width + tile_size - 1) // tile_size):
                xs = range(x0 + tx * tile_size, x0 + min(width, (tx + 1) * tile_size))
                ys = range(y0 + ty * tile_size, y0 + min(height, (ty + 1) * tile_size))
                data = pixel_rows(channels, x0, y0, xs, ys)
                chunks.append(struct.pack("<iiiii", tx, ty, 0, 0, len(data)) + data)
    else:
        for y in range(y0, y0 + height):
            data = pixel_rows(channels, x0, y0, range(x0, x0 + width), [y])
            chunks.append(struct.pack("<ii", y, len(data)) + data)

    offset = len(head) + 8 * len(chunks)
    table = bytearray()
    for chunk in chunks:
        table += struct.pack("<Q", offset)
        offset += len(chunk)
    with open(path, "wb") as file:
        file.write(head + table + b"".join(chunks))


def chunk_payloads(path):
    """The compressed payload of every chunk of a single-part file."""
    with open(path, "rb") as file:
        data = file.read()
    tiled = struct.unpack_from("<I", data, 4)[0] & 0x200
    position = 8
    while data[position] != 0:  # skip the attributes
        position = data.index(b"\0", position) + 1  # name
        position = data.index(b"\0", position) + 1  # type
        size = struct.unpack_from("<i", data, position)[0]
        position += 4 + size
    position += 1
    payloads = []
    # the offset table runs up to the first chunk
    first = struct.unpack_from("<Q", data, position)[0]
    for offset in struct.unpack_from("<%dQ" % ((first - position) // 8), data, position):
        start = offset + (20 if tiled else 8)
        size = struct.unpack_from("<i", data, start - 4)[0]
        payloads.append(data[start : start + size])
    return payloads


CASES = [
    # name, channels, data window origin and size, tile size
    ("rgba_half", [("R", HALF), ("G", HALF), ("B", HALF), ("A", HALF)], (-5, -9, 61, 43), None),
    (
        "rgb_float_uint",
        [("R", FLOAT), ("G", FLOAT), ("B", FLOAT), ("Z", FLOAT), ("id", UINT)],
        (3, 2, 37, 70),
        None,
    ),
    ("mixed_rgb", [("R", HALF), ("G", FLOAT), ("B", HALF)], (0, 0, 29, 44), None),
    ("y_half", [("Y", HALF)], (0, 0, 40, 48), None),
    ("tiled", [("R", HALF), ("G", HALF), ("B", HALF), ("Z", FLOAT)], (0, 0, 45, 44), 16),
]

for name, channels, (x0, y0, width, height), tile_size in CASES:
    source = name + ".exr"
    write_uncompressed(source, channels, x0, y0, width, height, tile_size)
    for method in ["htj2k32", "htj2k256"]:
        target = "%s_%s.exr" % (name, method)
        subprocess.run(["exrmetrics", "--convert", "-z", method, source, "-o", target], check=True)
        payloads = chunk_payloads(target)
        raw = [i for i, payload in enumerate(payloads) if payload[:2] != b"HT"]
        if raw:
            raise SystemExit("%s: chunks %s are not HTJ2K" % (target, raw))
        print("%s: %d chunks, %d bytes" % (target, len(payloads), os.path.getsize(target)))
