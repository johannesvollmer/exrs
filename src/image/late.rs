

/* TODO loads tiles from files where required
pub struct LateData<P, R: Read + Seek> {
    offset_table: OffsetTable,
    pixel_cache: Vec<P>,
    stream: R,
}

/// immediately loaded
pub struct FullData<P> {
    pixels: Vec<P>, // TODO per channel
}
impl<R: Read + Seek> Image<R> {
    pub fn load(mut source: R) -> crate::file::ReadResult<Self> {
        let meta_data = crate::file::io::read_meta_data(source)?;
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
            let chunk = crate::file::chunks::MultiPartChunk::read(&mut source, &self.meta_data)
                .unwrap();

            cache[&(part, tile_index)] = unpack_pixel_data(chunk)

        } else {
            let chunk: SinglePartChunk = crate::file::chunks::SinglePartChunks::read_chunk(&mut source, &self.meta_data)
                .unwrap();

            cache[&(part, tile_index)] = unpack_pixel_data(chunk)
        }
    }

    pub fn cache_chunk_for_pixel(&mut self, part: usize, pixel: (usize, usize)) {
        let dimensions =
        self.load_chunk(part, pixel.1 % self.meta_data.headers[part].data_window().width + pixel.0)
    }
}
*/
