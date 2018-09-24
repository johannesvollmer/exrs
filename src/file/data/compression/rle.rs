use super::*;

/*
TODO

pub fn decompress(target: UncompressedData, data: &CompressedData, uncompressed_size: Option<usize>, line_size: usize) -> Result<UncompressedData> {
    let mut decompressed = Vec::with_capacity(uncompressed_size.unwrap_or(32));

    {// decompress
        let mut decompressor = Decoder::new(data.as_slice())
            .expect("io error when reading from in-memory vec");

        decompressor.read_to_end(&mut decompressed)?;
    };

    integrate(&mut decompressed); // TODO per channel? per line??
    decompressed = reorder_decompress(&decompressed);
    super::uncompressed::unpack(target, &decompressed, line_size) // convert to machine-dependent endianess
}

pub fn compress(data: &UncompressedData) -> Result<CompressedData> {
    let mut packed = super::uncompressed::pack(data)?; // convert from machine-dependent endianess
    packed = reorder_compress(&packed);
    derive(&mut packed);

    {// compress
        let mut compressor = Encoder::new(Vec::with_capacity(128))
            .expect("io error when writing to in-memory vec");

        io::copy(&mut packed.as_slice(), &mut compressor).expect("io error when writing to in-memory vec");
        Ok(compressor.finish().into_result().expect("io error when writing to in-memory vec"))
    }
}*/
