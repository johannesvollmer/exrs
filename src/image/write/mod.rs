
pub mod layers;
pub mod options;
pub mod samples;
pub mod channels;




use crate::meta::Headers;
use crate::block::BlockIndex;
use crate::error::UnitResult;
use std::io::{Seek, BufWriter};
use crate::io::Write;
use crate::meta::header::{Header};
use crate::image::{Image, ignore_progress};
use crate::image::write::layers::{WritableLayers, LayersWriter};
// use crate::image::write::options::WriteImageWithProgress;


// extension for "Image" which allows calling ".write()...." on an image
pub trait WritableImage<'i, L>: Sized {
    fn write(self) -> WriteImageWithOptions<'i, L, fn(f64)>;
}

impl<'i, L: WritableLayers<'i>> WritableImage<'i, L> for &'i Image<L> {
    fn write(self) -> WriteImageWithOptions<'i, L, fn(f64)> {
        WriteImageWithOptions { image: self, pedantic: true, parallel: true, on_progress: ignore_progress }
    }
}


// temporary writer with options
#[derive(Debug, Clone, PartialEq)]
pub struct WriteImageWithOptions<'i, L, F> {
    image: &'i Image<L>,
    pedantic: bool,
    parallel: bool,
    on_progress: F,
}

/*pub trait WriteImage {
    fn is_pedantic(&self) -> bool;
    fn is_parallel(&self) -> bool;

    type Writer: ImageWriter;
    fn infer_meta_data(&self) -> Headers;
    fn create_image_writer(&self, headers: &[Header]) -> Self::Writer;
}*/

/*
impl<I> WriteImageToDestination for I where I: WriteImage {
    fn to_buffered(&self, write: impl Write + Seek) -> UnitResult {

    }
}
*/

impl<'i, L, F> WriteImageWithOptions<'i, L, F> where L: WritableLayers<'i>, F: FnMut(f64) {
    // type Writer = ImageWithOptionsWriter<L::Writer>;

    pub fn infer_meta_data(&self) -> Headers {
        self.image.layer_data.infer_headers(&self.image.attributes)
    }

    fn create_image_writer(&self, headers: &[Header]) -> ImageWithOptionsWriter<L::Writer> {
        ImageWithOptionsWriter { layers: self.image.layer_data.create_writer(headers) }
    }

    pub fn pedantic(self) -> Self { Self { pedantic: true, ..self } }
    pub fn non_parallel(self) -> Self { Self { parallel: false, ..self } }
    pub fn on_progress(self, on_progress: F) -> Self where F: FnMut(f64) { Self { on_progress, ..self } }

    // pub fn without_image_validation(self) -> Self { Self { pedantic: false, ..self  } }
    // pub fn non_parallel(self) -> Self { Self { parallel: false, ..self  } }
    /* TODO would need mutable `extract_block` signature
         pub fn on_progress<F>(self, on_progress: F) -> WriteImageWithProgress<Self, F> where F: FnMut(f64) {
        WriteImageWithProgress { inner: self, on_progress }
    }*/

    /// Write the exr image to a file.
    /// Use `write_to_unbuffered` instead if you do not have a file.
    /// If an error occurs, attempts to delete the partially written file.
    #[inline]
    #[must_use]
    pub fn to_file(self, path: impl AsRef<std::path::Path>) -> UnitResult {
        crate::io::attempt_delete_file_on_write_error(path.as_ref(), move |write|
            self.to_unbuffered(write)
        )
    }

    /// Buffer the writer and then write the exr image to it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first, using `write_to_buffered`.
    #[inline]
    #[must_use]
    pub fn to_unbuffered(self, unbuffered: impl Write + Seek) -> UnitResult {
        self.to_buffered(BufWriter::new(unbuffered))
    }

    /// Write the exr image to a writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory writer.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first.
    #[must_use]
    pub fn to_buffered(self, write: impl Write + Seek) -> UnitResult {
        let meta_data = self.infer_meta_data(); // TODO non-failing gen_meta?
        let writer = self.create_image_writer(&meta_data);

        crate::block::write_all_blocks_to_buffered(
            write, meta_data,
            move |meta, block| { writer.extract_uncompressed_block(meta, block) },
            self.on_progress, self.pedantic, self.parallel,
        )
    }
}
/*
impl<'i, L> WriteImageWithOptions<'i, L, F> where L: WritableLayers<'i> {
    pub fn without_image_validation(self) -> Self { Self { pedantic: false, ..self  } }
    pub fn non_parallel(self) -> Self { Self { parallel: false, ..self  } }
    /* TODO would need mutable `extract_block` signature
         pub fn on_progress<F>(self, on_progress: F) -> WriteImageWithProgress<Self, F> where F: FnMut(f64) {
        WriteImageWithProgress { inner: self, on_progress }
    }*/
}*/

// TODO remove intermediate struct!
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImageWithOptionsWriter<L> {
    layers: L, // impl LayersWriter
}

pub trait ImageWriter: Sync {
    fn extract_uncompressed_block(&self, headers: &[Header], block: BlockIndex) -> Vec<u8>;
}

impl<L> ImageWriter for ImageWithOptionsWriter<L> where L: LayersWriter {
    fn extract_uncompressed_block(&self, headers: &[Header], block: BlockIndex) -> Vec<u8> {
        self.layers.extract_uncompressed_block(headers, block)
    }
}

/*
struct WriteImageWithProgress<I, F> {
    inner: I, // impl WriteImage
    on_progress: F, // impl FnMut(f64)
}

struct OnProgressImageWriter<I, F> {
    inner: I, // impl ImageWriter
    on_progress: F, // impl FnMut(f64)
    total_blocks: usize,
    processed_blocks: usize,
}

impl<I, F> WriteImage for WriteImageWithProgress<I, F> where I: ImageWriter, F: FnMut(f64) {
    type Writer = OnProgressImageWriter<I, F>;

    fn infer_meta_data(&mut self) -> Headers {
        self.inner.infer_meta_data()
    }

    fn create_image_writer(self, headers: &[Header]) -> Self::Writer {
        OnProgressImageWriter {
            inner: self.inner.create_image_writer(headers),
            on_progress: self.on_progress,
            total_blocks: headers.iter().map(|header| header.chunk_count).sum(), // TODO filtered?
            processed_blocks: 0
        }
    }
}

impl<I, F> ImageWriter for OnProgressImageWriter<I, F> where I: ImageWriter, F: FnMut(f64) {
    fn extract_uncompressed_block(&mut self, headers: &[Header], block: BlockIndex) -> Vec<u8> {
        let block = self.inner.extract_uncompressed_block(headers, block);

        self.processed_blocks += 1;
        let function = &mut self.on_progress;
        function(self.processed_blocks as f64 / self.total_blocks as f64);

        block
    }
}



trait WritableLayers {
    fn infer_headers(&self, image_attributes: &ImageAttributes) -> Headers;

    type Writer: LayersWriter;
    fn create_writer(&self, headers: &[Header]) -> Self::Write;
}

trait LayersWriter {
    fn extract_uncompressed_block(&mut self, headers: &[Header], block: BlockIndex) -> Vec<u8>;
}


struct AllLayersWriter<'a, C> {
    layers: SmallVec<[LayerWriter<'a, C>; 2]>
}

struct LayerWriter<'a, C> {
    channels: C, // impl ChannelsWriter
    attributes: &'a LayerAttributes,
}

impl<'l, C> WritableLayers for &'l Layers<C> where C: WritableChannels {
    fn infer_headers(&self, image_attributes: &ImageAttributes) -> Headers {
        self.iter().map(|layer| layer.infer_headers(image_attributes).first().unwrap()).collect() // TODO no array-vs-first
    }

    type Writer = AllLayersWriter<'l, C::Write>;
    fn create_writer(&self, headers: &[Header]) -> Self::Write {
        AllLayersWriter {
            layers: self.iter().zip(headers.chunks_exact(1)) // TODO no array-vs-first
                .map(|(layer, header)| layer.create_writer(header))
                .collect()
        }
    }
}

impl<'l, C> WritableLayers for &'l Layer<C> where C: WritableChannels {
    fn infer_headers(&self, image_attributes: &ImageAttributes) -> Headers {
        self.iter().map(|layer| layer.infer_headers(image_attributes).first().unwrap()).collect() // TODO no array-vs-first
    }

    type Writer = LayerWriter<'l, C::Write>;
    fn create_writer(&self, headers: &[Header]) -> Self::Write {
        LayerWriter {
            channels: self.channel_data.create_writer(headers.first().unwrap()), // TODO no array-vs-first
            attributes: &self.attributes
        }
    }
}*/




mod test {
    // use crate::prelude::*;
/*
    #[test]
    fn compiles() {
        let (width, height) = (1024, 512);
        let red_samples: Vec<f16> = vec![ f16::PI; width * height ];
        let alpha_samples: Vec<f16> = vec![ f16::ONE; width * height ];

        let image: Image<Layer<AnyChannels<FlatSamples>>> = Image::from_single_layer( // FIXME do not require borrowing
            Layer::new(
                (width, height),
                LayerAttributes::named("Layer".try_into().unwrap()),
                Encoding::FAST_LOSSLESS,
                smallvec![
                    AnyChannel::luminance_based("R".try_into().unwrap(), FlatSamples::F16(red_samples)),  // FIXME do not require borrowing
                    AnyChannel::non_luminance_based("A".try_into().unwrap(), FlatSamples::F16(alpha_samples)),  // FIXME do not require borrowing
                ]
            )
        );

        image.write()
            // TODO .on_progress(|| println!("made progress!"))
            .to_file("exr.exr");


        let pixel_data: Vec<f32> = vec![ 0.3; width * height ];

        let my_pixels = |position: Vec2<usize>| -> RgbaPixel {
            let value = pixel_data[position.flatten_for_width(width)];
            RgbaPixel::new(value, value, value, Some(value))
        };

        let image = Image::new(
            ImageAttributes::with_size((width, height)),
            smallvec![ // FIXME do not require borrowing
                Layer::new(
                    (width, height),
                    LayerAttributes::named("layer1".try_into().unwrap()),
                    Encoding::FAST_LOSSLESS,
                    RgbaChannels::new(RgbaSampleTypes::RGBA_F16, &my_pixels) // FIXME do not require borrow
                ),

                Layer::new(
                    (width, height),
                    LayerAttributes::named("layer1".try_into().unwrap()),
                    Encoding::SMALL_FAST_LOSSY,
                    RgbaChannels::new(RgbaSampleTypes::RGBA_F16, &my_pixels) // FIXME do not require borrow
                ),
            ]
        );

        image.write()
            .without_image_validation()
            .to_file("exr.exr");

    }*/
}