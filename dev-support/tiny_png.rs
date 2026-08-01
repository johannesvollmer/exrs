// A minimal PNG reader/writer for tests and examples, built on `flate2`
// (already a dependency of `exr` via the `zlib-rs` feature) instead of the
// `image`/`png` crates.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

fn crc32(data: &[u8]) -> u32 {
    const fn make_table() -> [u32; 256] {
        let mut table = [0u32; 256];
        let mut n = 0;
        while n < 256 {
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            table[n] = c;
            n += 1;
        }
        table
    }

    const TABLE: [u32; 256] = make_table();

    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = TABLE[index] ^ (crc >> 8);
    }

    crc ^ 0xFFFFFFFF
}

fn write_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&out[start..]).to_be_bytes());
}

fn encode_png(
    path: impl AsRef<Path>, width: u32, height: u32, color_type: u8, bytes_per_pixel: usize,
    raw: &[u8],
) -> io::Result<()> {
    let row_len = width as usize * bytes_per_pixel;
    let mut filtered = Vec::with_capacity(raw.len() + height as usize);
    for row in raw.chunks_exact(row_len) {
        filtered.push(0u8); // filter type: None
        filtered.extend_from_slice(row);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&filtered)?;
    let compressed = encoder.finish()?;

    let mut out = Vec::new();
    out.extend_from_slice(&SIGNATURE);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(color_type);
    ihdr.push(0); // compression method
    ihdr.push(0); // filter method
    ihdr.push(0); // interlace method
    write_chunk(&mut out, b"IHDR", &ihdr);
    write_chunk(&mut out, b"IDAT", &compressed);
    write_chunk(&mut out, b"IEND", &[]);

    File::create(path)?.write_all(&out)
}

/// Encode an 8-bit grayscale image (one byte per pixel) as a PNG.
pub fn write_gray8(path: impl AsRef<Path>, width: u32, height: u32, pixels: &[u8]) -> io::Result<()> {
    assert_eq!(pixels.len(), width as usize * height as usize, "pixel buffer size mismatch");
    encode_png(path, width, height, 0, 1, pixels)
}

/// Encode an 8-bit RGBA image as a PNG.
pub fn write_rgba8(
    path: impl AsRef<Path>, width: u32, height: u32, pixels: &[[u8; 4]],
) -> io::Result<()> {
    assert_eq!(pixels.len(), width as usize * height as usize, "pixel buffer size mismatch");
    let raw: Vec<u8> = pixels.iter().flatten().copied().collect();
    encode_png(path, width, height, 6, 4, &raw)
}

/// A minimal in-memory 16-bit RGB image buffer, standing in for
/// `image::ImageBuffer<image::Rgb<u16>, Vec<u16>>`.
pub struct Rgb16Buffer {
    width: u32,
    height: u32,
    data: Vec<[u16; 3]>,
}

impl Rgb16Buffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, data: vec![[0u16; 3]; width as usize * height as usize] }
    }

    pub fn put_pixel(&mut self, x: u32, y: u32, pixel: [u16; 3]) {
        self.data[y as usize * self.width as usize + x as usize] = pixel;
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn pixels(&self) -> impl Iterator<Item = &[u16; 3]> {
        self.data.iter()
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

fn read_u32_be(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (a as i32, b as i32, c as i32);
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());

    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

/// Decode a non-interlaced, 16-bit, truecolor-RGB PNG (color type 2). This is
/// the only shape of PNG this repo needs to read.
pub fn read_rgb16(path: impl AsRef<Path>) -> io::Result<Rgb16Buffer> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;

    if bytes.len() < 8 || bytes[..8] != SIGNATURE {
        return Err(invalid("not a PNG file"));
    }

    let mut pos = 8;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut idat = Vec::new();
    let mut seen_ihdr = false;

    while pos + 8 <= bytes.len() {
        let len = read_u32_be(&bytes[pos..pos + 4]) as usize;
        let chunk_type = &bytes[pos + 4..pos + 8];
        let data_start = pos + 8;
        let data_end =
            data_start.checked_add(len).ok_or_else(|| invalid("PNG chunk length overflow"))?;
        if data_end + 4 > bytes.len() {
            return Err(invalid("truncated PNG chunk"));
        }
        let data = &bytes[data_start..data_end];

        match chunk_type {
            b"IHDR" => {
                if data.len() != 13 {
                    return Err(invalid("malformed IHDR"));
                }
                width = read_u32_be(&data[0..4]);
                height = read_u32_be(&data[4..8]);
                let (bit_depth, color_type, interlace) = (data[8], data[9], data[12]);
                if bit_depth != 16 || color_type != 2 || interlace != 0 {
                    return Err(invalid(
                        "only non-interlaced 16-bit truecolor RGB PNGs are supported",
                    ));
                }
                seen_ihdr = true;
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {} // ancillary chunk (gAMA, pHYs, tEXt, ...): ignore
        }

        pos = data_end + 4; // skip the trailing CRC
    }

    if !seen_ihdr {
        return Err(invalid("missing IHDR chunk"));
    }

    let mut raw = Vec::new();
    ZlibDecoder::new(&idat[..]).read_to_end(&mut raw)?;

    const BYTES_PER_PIXEL: usize = 6; // 3 channels * 16 bits
    let row_len = width as usize * BYTES_PER_PIXEL;
    if raw.len() != (row_len + 1) * height as usize {
        return Err(invalid("unexpected decompressed PNG data size"));
    }

    let mut buffer = Rgb16Buffer::new(width, height);
    let mut prior_row = vec![0u8; row_len];

    for y in 0..height as usize {
        let row_start = y * (row_len + 1);
        let filter_type = raw[row_start];
        let mut row = raw[row_start + 1..row_start + 1 + row_len].to_vec();

        for i in 0..row_len {
            let a = if i >= BYTES_PER_PIXEL { row[i - BYTES_PER_PIXEL] } else { 0 };
            let b = prior_row[i];
            let c = if i >= BYTES_PER_PIXEL { prior_row[i - BYTES_PER_PIXEL] } else { 0 };

            row[i] = match filter_type {
                0 => row[i],
                1 => row[i].wrapping_add(a),
                2 => row[i].wrapping_add(b),
                3 => row[i].wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => row[i].wrapping_add(paeth_predictor(a, b, c)),
                _ => return Err(invalid("unsupported PNG filter type")),
            };
        }

        for x in 0..width as usize {
            let p = x * BYTES_PER_PIXEL;
            buffer.put_pixel(
                x as u32,
                y as u32,
                [
                    u16::from_be_bytes([row[p], row[p + 1]]),
                    u16::from_be_bytes([row[p + 2], row[p + 3]]),
                    u16::from_be_bytes([row[p + 4], row[p + 5]]),
                ],
            );
        }

        prior_row = row;
    }

    Ok(buffer)
}
