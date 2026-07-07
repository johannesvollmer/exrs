#!/usr/bin/env python3
# Regenerates the DWA cross-check fixtures in this directory using the real
# OpenEXR C++ library (via its Python bindings), to verify that exrs's DWA
# decoder is bit-identical to the reference implementation for scenarios with
# more than one LOSSY_DCT channel group per chunk:
#   - y_ry_by_dwaa.exr:  three standalone LOSSY_DCT channels (Y, RY, BY).
#                        None of these are ever CSC-grouped (cscIdx == -1
#                        for all three in internal_dwa_classifier.h), so this
#                        also checks that decoding three standalone groups in
#                        sequence reads the right slice of the DC buffer for
#                        each one.
#   - rgb_plus_y_dwaa.exr: an R/G/B CSC triplet *followed by* a standalone Y
#                          channel, to check the transition from a 3-component
#                          CSC group's DC cursor advance to a subsequent
#                          standalone group's.
# (Legacy lowercase names like "red"/"grn"/"blue" are intentionally not
# covered here: the real encoder's default classifier table only matches
# exact-case "R"/"G"/"B"/"Y"/"BY"/"RY"/"A", so such channels are written with
# CompressorScheme::Unknown, not LOSSY_DCT - lowercase legacy names are only
# a *decode*-time compatibility path for version<2 files from historical
# encoders, which no current tool produces.)
# Each *_dwaa.exr is paired with a *_ground_truth.exr: an uncompressed EXR
# holding the real library's own decode of the just-written DWAA file. The
# Rust test reads both through the normal EXR path and reuses exrs's
# existing lossy-compression ValidateResult tolerance (see
# tests/across_compression.rs), so there's no bespoke binary fixture format
# and no Python/OpenEXR runtime dependency for the test itself.
import numpy as np
import OpenEXR
import Imath

W, H = 64, 48
HALF = Imath.PixelType(Imath.PixelType.HALF)


def make_plane(kind, seed):
    rng = np.random.default_rng(seed)
    xs, ys = np.meshgrid(np.linspace(0, 1, W), np.linspace(0, 1, H))
    if kind == "y":
        base = 0.2 + 0.6 * xs
    elif kind == "ry":
        base = 0.1 * np.sin(3 * np.pi * ys)
    elif kind == "by":
        base = 0.1 * np.cos(3 * np.pi * xs)
    else:
        base = 0.5 * xs + 0.5 * ys
    noise = rng.normal(scale=0.01, size=(H, W))
    return (base + noise).astype(np.float32)


def write_and_dump(path_exr, path_ground_truth_exr, channel_names, planes):
    header = OpenEXR.Header(W, H)
    header['channels'] = {name: Imath.Channel(HALF) for name in channel_names}
    header['compression'] = Imath.Compression(Imath.Compression.DWAA_COMPRESSION)

    half_planes = {
        name: plane.astype(np.float16) for name, plane in zip(channel_names, planes)
    }

    out = OpenEXR.OutputFile(path_exr, header)
    out.writePixels({name: half_planes[name].tobytes() for name in channel_names})
    out.close()

    # Ground truth: decode the file we just wrote with the real library, and
    # re-save it uncompressed so the Rust test can load it like any other EXR.
    inp = OpenEXR.InputFile(path_exr)
    ground_truth_pixels = {name: inp.channel(name, HALF) for name in channel_names}
    inp.close()

    ground_truth_header = OpenEXR.Header(W, H)
    ground_truth_header['channels'] = {name: Imath.Channel(HALF) for name in channel_names}
    ground_truth_header['compression'] = Imath.Compression(Imath.Compression.NO_COMPRESSION)

    ground_truth_out = OpenEXR.OutputFile(path_ground_truth_exr, ground_truth_header)
    ground_truth_out.writePixels(ground_truth_pixels)
    ground_truth_out.close()


write_and_dump(
    "y_ry_by_dwaa.exr", "y_ry_by_dwaa_ground_truth.exr",
    ["Y", "RY", "BY"],
    [make_plane("y", 1), make_plane("ry", 2), make_plane("by", 3)],
)

write_and_dump(
    "rgb_plus_y_dwaa.exr", "rgb_plus_y_dwaa_ground_truth.exr",
    ["R", "G", "B", "Y"],
    [make_plane("r", 4), make_plane("g", 5), make_plane("b", 6), make_plane("y", 7)],
)

print("done")
