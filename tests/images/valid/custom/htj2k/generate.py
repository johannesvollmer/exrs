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
#
# LJ2K, lossy HTJ2K, is generated too (OpenEXR 3.5.3 was used; for the
# LJ2K of 3.5.0 and 3.5.1, see the end of this comment). tests/lj2k.rs
# reads it:
# `<name>_lj2k_q<Q>.exr` is `<name>.exr` converted with `-z lj2k -l Q` (a
# "." in Q becomes "_"), and `<name>_lj2k_q<Q>_decoded.exr` is that file
# as OpenEXR decodes it, converted to ZIP, which is lossless and keeps the
# file small. The test decodes each LJ2K file with exrs and compares every
# sample with OpenEXR's decoding: exactly in the channels LJ2K codes
# losslessly, and to within one unit in the last place in the lossy ones,
# since the OpenJPH that OpenEXR links rounds some float conversions
# differently on CPUs with AVX2 (see tests/lj2k_common).
#
# LJ2K codes R, G and B lossily only when they are HALF or FLOAT, of one
# type and not subsampled; it codes every other channel losslessly. The
# LJ2K cases reuse three sources above:
#   rgba_half          lossy half RGB with specials and a lossless A, at a
#                      negative origin
#   mixed_rgb          RGB of mixed types: nothing lossy
#   y_half             no RGB: nothing lossy
# and add these, with no specials but in rgb_float_specials:
#   rgb_half           half RGB 9x270: two chunks, the second short (and
#                      stored raw); every class of quality: Qfactor below
#                      97, a fractional one that OpenEXR rounds, explicit
#                      steps from 97, and 150, at which every chunk is raw
#   rgb_float_specials float RGB with specials, among them the largest
#                      finite floats, so that OpenEXR builds the float table
#                      for the chunk's range; a lossless float Z and UINT id
#   rgb_float_hdr      float RGB up to about 3.5e5, past half's 65504, so
#                      that the float table spans a moderate range
#   mixed_types        half RGB, float Z, UINT id and half A
#   extra_channels     a lossy RGB triplet among other colour channels, a
#                      lossless "diffuse" triplet of floats, an id, normals
#   width_33           half RGB 33 wide, which needs no column padding
#   tiled_odd          half RGBA in 32x32 tiles over 40x36 at an odd origin;
#                      the partial tiles are stored raw
#
# Legacy LJ2K: OpenEXR 3.5.0 and 3.5.1 wrote LJ2K chunks with the "HT"
# magic number, and coded their lossy R, G and B samples after mapping
# them through the transfer function to integers, with no NLT table in the
# codestream; OpenEXR 3.5.2 and later undo that mapping when they read such
# chunks. (3.5.0 and 3.5.1 have the same HTJ2K code.) These files need
# OpenEXR 3.5.1's exrmetrics, given by the environment variable
# EXRMETRICS_351; without it the script skips them. The tests need only
# the committed files. For each case below:
#   <name>_lj2k351_q<Q>.exr          <name>.exr converted to LJ2K at
#                                    quality Q by 3.5.1's exrmetrics
#   <name>_lj2k351_q<Q>_decoded.exr  that file decoded by `exrmetrics`
#                                    (3.5.3), converted to ZIP
# The script also decodes each file uncompressed with both exrmetrics and
# stops unless the two outputs are the same bytes, so the one decoded file
# stands for both. legacy_versions.txt records the `--version` line of each
# exrmetrics. The 3.5.1 used was built from its release sources with its
# bundled OpenJPH 0.32.0, statically, SIMD enabled. tests/lj2k_legacy.rs
# compares exrs's decoding of each file with the decoded file. The cases
# reuse the sources above (3.5.1's exrmetrics cannot read the tiled one
# without a "type" attribute, so it gets a copy with one):
#   rgb_half           half RGB, two chunks; Qfactor 1, 50.5 (rounded),
#                      97 and 110 (explicit steps)
#   rgba_half          half RGB with specials (the encoder maps NaN to 0 and
#                      infinities to the largest code) and a lossless A
#   rgb_float_specials float RGB with specials and the largest finite
#                      floats, a lossless float Z and UINT id
#   rgb_float_hdr      float RGB beyond half's range
#   mixed_types        half RGB, float Z, UINT id and half A
#   mixed_rgb, y_half  nothing lossy: RGB of mixed types, and no RGB
#   extra_channels     a lossy triplet among lossless colour channels
#   width_33, tiled_odd  odd sizes, and tiles with partial ones raw
import os
import struct
import subprocess
import sys

HALF, FLOAT, UINT = 1, 2, 0

HALF_SPECIALS = [0x7C00, 0xFC00, 0x7E01, 0xFD23, 0x8000, 0x0001, 0x83FF, 0x7BFF, 0xFBFF, 0x0000]
FLOAT_SPECIALS = [
    0x7F800000, 0xFF800000, 0x7FC00001, 0xFFA5A5A5, 0x80000000,
    0x00000001, 0x807FFFFF, 0x7F7FFFFF, 0xFF7FFFFF, 0x00000000,
]
UINT_SPECIALS = [0xFFFFFFFF, 0x80000000, 0x7FFFFFFF, 0x00000000, 0x00000001]


def sample_bytes(kind, channel_index, x, y, pattern="smooth"):
    """One sample, little-endian: a ramp, or now and then a special value.
    Pattern "clean" has no specials, and "hdr" neither, with FLOAT samples
    that grow past half's range."""
    if pattern == "hdr" and kind == FLOAT:
        value = (1.0 + x * x) * (1.0 + y) * (1.5 + 0.25 * channel_index) * 20.0
        return struct.pack("<f", value)
    special = pattern == "smooth" and (x * 7 + y * 13 + channel_index * 5) % 193 == 0
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


def header(channels, x0, y0, width, height, tile_size, typed=False):
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
        if typed:
            # optional in a single-part file, but 3.5.1's exrmetrics needs it
            attributes.append(attribute("type", "string", b"tiledimage"))
    return b"\x76\x2f\x31\x01" + struct.pack("<I", flags) + b"".join(attributes) + b"\0"


def pixel_rows(channels, x0, y0, x_range, y_range, pattern="smooth"):
    """The uncompressed data of a block: line by line, channels in order."""
    data = bytearray()
    for y in y_range:
        for index, (_, kind) in enumerate(channels):
            for x in x_range:
                data += sample_bytes(kind, index, x - x0, y - y0, pattern)
    return bytes(data)


def write_uncompressed(
    path, channels, x0, y0, width, height, tile_size=None, pattern="smooth", typed=False
):
    channels = sorted(channels)
    head = header(channels, x0, y0, width, height, tile_size, typed)
    chunks = []
    if tile_size:
        for ty in range((height + tile_size - 1) // tile_size):
            for tx in range((width + tile_size - 1) // tile_size):
                xs = range(x0 + tx * tile_size, x0 + min(width, (tx + 1) * tile_size))
                ys = range(y0 + ty * tile_size, y0 + min(height, (ty + 1) * tile_size))
                data = pixel_rows(channels, x0, y0, xs, ys, pattern)
                chunks.append(struct.pack("<iiiii", tx, ty, 0, 0, len(data)) + data)
    else:
        for y in range(y0, y0 + height):
            data = pixel_rows(channels, x0, y0, range(x0, x0 + width), [y], pattern)
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

# `python3 generate.py --legacy-only` writes only the sources and the
# legacy LJ2K files.
CURRENT = "--legacy-only" not in sys.argv[1:]

for name, channels, (x0, y0, width, height), tile_size in CASES:
    source = name + ".exr"
    write_uncompressed(source, channels, x0, y0, width, height, tile_size)
    for method in ["htj2k32", "htj2k256"] if CURRENT else []:
        target = "%s_%s.exr" % (name, method)
        subprocess.run(["exrmetrics", "--convert", "-z", method, source, "-o", target], check=True)
        payloads = chunk_payloads(target)
        raw = [i for i, payload in enumerate(payloads) if payload[:2] != b"HT"]
        if raw:
            raise SystemExit("%s: chunks %s are not HTJ2K" % (target, raw))
        print("%s: %d chunks, %d bytes" % (target, len(payloads), os.path.getsize(target)))

LJ2K_SOURCES = [
    # name, channels, data window origin and size, tile size, sample pattern
    ("rgb_half", [("R", HALF), ("G", HALF), ("B", HALF)], (0, 0, 9, 270), None, "clean"),
    (
        "rgb_float_specials",
        [("R", FLOAT), ("G", FLOAT), ("B", FLOAT), ("Z", FLOAT), ("id", UINT)],
        (2, 3, 16, 24),
        None,
        "smooth",
    ),
    ("rgb_float_hdr", [("R", FLOAT), ("G", FLOAT), ("B", FLOAT)], (0, 0, 20, 24), None, "hdr"),
    (
        "mixed_types",
        [("R", HALF), ("G", HALF), ("B", HALF), ("Z", FLOAT), ("id", UINT), ("A", HALF)],
        (0, 0, 16, 20),
        None,
        "clean",
    ),
    (
        "extra_channels",
        [
            ("A", HALF), ("B", HALF), ("G", HALF), ("R", HALF),
            ("diffuse.blue", FLOAT), ("diffuse.green", FLOAT), ("diffuse.red", FLOAT),
            ("id", UINT), ("normal.x", HALF),
        ],
        (0, 0, 20, 24),
        None,
        "clean",
    ),
    ("width_33", [("R", HALF), ("G", HALF), ("B", HALF)], (0, 0, 33, 40), None, "clean"),
    (
        "tiled_odd",
        [("R", HALF), ("G", HALF), ("B", HALF), ("A", HALF)],
        (3, -7, 40, 36),
        32,
        "clean",
    ),
]

LJ2K_QUALITIES = {
    "rgba_half": [110],
    "mixed_rgb": [110],
    "y_half": [110],
    "rgb_half": [1, 50.5, 96, 97, 110, 150],
    "rgb_float_specials": [110],
    "rgb_float_hdr": [50, 97, 110],
    "mixed_types": [96],
    "extra_channels": [110],
    "width_33": [97],
    "tiled_odd": [110, 150],
}

# Lossy chunks are larger than the samples more often than lossless ones:
# padding a small chunk to 33 lines or columns costs more than it saves,
# and so do quality levels near 150 with HALF samples. OpenEXR stores those
# chunks raw. That is fine in some chunks of a file, but every file except
# these must have at least one LJ2K chunk.
ALL_RAW = {"rgb_half_lj2k_q150.exr"}

for name, channels, (x0, y0, width, height), tile_size, pattern in LJ2K_SOURCES:
    write_uncompressed(name + ".exr", channels, x0, y0, width, height, tile_size, pattern)

for name, qualities in LJ2K_QUALITIES.items() if CURRENT else []:
    source = name + ".exr"
    for quality in qualities:
        target = "%s_lj2k_q%s.exr" % (name, ("%g" % quality).replace(".", "_"))
        decoded = target[: -len(".exr")] + "_decoded.exr"
        convert = ["exrmetrics", "--convert", "-z", "lj2k", "-l", "%g" % quality, source]
        subprocess.run(convert + ["-o", target], check=True)
        subprocess.run(["exrmetrics", "--convert", "-z", "zip", target, "-o", decoded], check=True)
        payloads = chunk_payloads(target)
        coded = sum(payload[:2] == b"HL" for payload in payloads)
        if coded == 0 and target not in ALL_RAW:
            raise SystemExit("%s: no chunk is LJ2K" % target)
        print(
            "%s: %d chunks, %d of them LJ2K, the others raw; %d bytes, decoded %d bytes"
            % (target, len(payloads), coded, os.path.getsize(target), os.path.getsize(decoded))
        )

LEGACY_QUALITIES = {
    "rgb_half": [1, 50.5, 97, 110],
    "rgba_half": [110],
    "rgb_float_specials": [110],
    "rgb_float_hdr": [50, 110],
    "mixed_types": [96],
    "mixed_rgb": [110],
    "y_half": [110],
    "extra_channels": [110],
    "width_33": [97],
    "tiled_odd": [110],
}

EXRMETRICS_351 = os.environ.get("EXRMETRICS_351")


def version(exrmetrics):
    """The first line of `exrmetrics --version`."""
    output = subprocess.run([exrmetrics, "--version"], check=True, capture_output=True, text=True)
    return output.stdout.splitlines()[0]


if not EXRMETRICS_351:
    print("EXRMETRICS_351 is not set: legacy LJ2K files skipped")
else:
    writer = version(EXRMETRICS_351)
    if " 3.5.1 " not in writer:
        raise SystemExit("EXRMETRICS_351 is %s" % writer)
    with open("legacy_versions.txt", "w") as file:
        file.write("The exrmetrics that made the *_lj2k351_* files (see generate.py):\n")
        file.write("*_lj2k351_q*.exr: %s\n" % writer)
        file.write("*_lj2k351_q*_decoded.exr: %s\n" % version("exrmetrics"))
        file.write("  (and 3.5.1 decodes each file to the same uncompressed bytes)\n")

    sources = {case[0]: case for case in LJ2K_SOURCES}
    for name, qualities in LEGACY_QUALITIES.items():
        source = name + ".exr"
        if name in sources and sources[name][3]:
            # a tiled source again, with a "type" attribute, for 3.5.1
            _, channels, (x0, y0, width, height), tile_size, pattern = sources[name]
            source = name + "_typed.exr"
            write_uncompressed(source, channels, x0, y0, width, height, tile_size, pattern, True)
        for quality in qualities:
            target = "%s_lj2k351_q%s.exr" % (name, ("%g" % quality).replace(".", "_"))
            stem = target[: -len(".exr")]
            convert = [EXRMETRICS_351, "--convert", "-z", "lj2k", "-l", "%g" % quality, source]
            subprocess.run(convert + ["-o", target], check=True)
            # both decode it to the same file, which is kept as ZIP
            decoded = []
            for exrmetrics in ["exrmetrics", EXRMETRICS_351]:
                subprocess.run([exrmetrics, "--convert", "-z", "none", target, "-o", "decoded.tmp"], check=True)
                with open("decoded.tmp", "rb") as file:
                    decoded.append(file.read())
            os.remove("decoded.tmp")
            if decoded[0] != decoded[1]:
                raise SystemExit("%s: OpenEXR 3.5.3 and 3.5.1 decode it differently" % target)
            convert = ["exrmetrics", "--convert", "-z", "zip", target, "-o", stem + "_decoded.exr"]
            subprocess.run(convert, check=True)
            payloads = chunk_payloads(target)
            if any(payload[:2] == b"HL" for payload in payloads):
                raise SystemExit("%s: a chunk has the magic number of 3.5.2 and later" % target)
            coded = sum(payload[:2] == b"HT" for payload in payloads)
            if coded == 0:
                raise SystemExit("%s: no chunk is LJ2K" % target)
            print(
                "%s: %d chunks, %d of them LJ2K, the others raw; %d bytes"
                % (target, len(payloads), coded, os.path.getsize(target))
            )
        if source != name + ".exr":
            os.remove(source)
