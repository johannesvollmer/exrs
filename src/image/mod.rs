
//! The `image` module is for interpreting the loaded file data.
//!

/*use ::std::io::{Read, Seek, SeekFrom};
use ::file::*;

pub struct Image<R: Read + Seek> {
    meta_data: MetaData,

    source: R,
    data: Pixels,
}



pub type Pixels = Vec<PixelBlock>;
pub struct PixelBlock;


impl<R: Read + Seek> Image<R> {
    pub fn load(mut source: R) -> ::file::ReadResult<Self> {
        let meta_data = ::file::io::read_meta_data(source)?;
        Ok(Image {
            source,
            meta_data,
            data: Vec::new(),
        })
    }

    pub fn load_chunk(&mut self, part: usize, tile_index: usize) {
        let offset_table = &self.meta_data.offset_tables[part];

        // go to start of chunk
        self.source.seek(SeekFrom::Start(offset_table[tile_index]))
            .unwrap();

        if self.meta_data.version.has_multiple_parts {
            let chunk = ::file::chunks::MultiPartChunk::read(&mut source, &self.meta_data)
                .unwrap();

            cache[&(part, tile_index)] = unpack_pixel_data(chunk)

        } else {
            let chunk: SinglePartChunk = ::file::chunks::SinglePartChunks::read_chunk(&mut source, &self.meta_data)
                .unwrap();

            cache[&(part, tile_index)] = unpack_pixel_data(chunk)
        }
    }

    pub fn cache_chunk_for_pixel(&mut self, part: usize, pixel: (usize, usize)) {
        let dimensions =
        self.load_chunk(part, pixel.1 % self.meta_data.headers[part].data_window().width + pixel.0)
    }
}

pub struct Part {
    header: Header,
    offset_table: OffsetTable,
}*/

//  The representation of 16-bit floating-point numbers is analogous to IEEE 754,
//  but with 5 exponent bits and 10 bits for the fraction