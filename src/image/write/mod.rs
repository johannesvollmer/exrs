
pub mod layers;
pub mod samples;
pub mod channels;



use crate::meta::Headers;
use crate::error::UnitResult;
use std::io::{Seek, BufWriter};
use crate::io::Write;
use crate::image::{Image, ignore_progress};
use crate::image::write::layers::{WritableLayers, LayersWriter};


// extension for "Image" which allows calling ".write()...." on an image
pub trait WritableImage<'i, L>: Sized {
    fn write(self) -> WriteImageWithOptions<'i, L, fn(f64)>;
}

impl<'i, L> WritableImage<'i, L> for &'i Image<L> {
    fn write(self) -> WriteImageWithOptions<'i, L, fn(f64)> {
        WriteImageWithOptions {
            image: self,
            check_compatibility: true, parallel: true, on_progress: ignore_progress
        }
    }
}


// temporary writer with options
#[derive(Debug, Clone, PartialEq)]
pub struct WriteImageWithOptions<'i, L, F> {
    image: &'i Image<L>,
    check_compatibility: bool,
    parallel: bool,
    on_progress: F,
}


impl<'i, L, F> WriteImageWithOptions<'i, L, F> where L: WritableLayers<'i>, F: FnMut(f64) {
    pub fn infer_meta_data(&self) -> Headers { // TODO this should perform all validity checks? and none after that?
        self.image.layer_data.infer_headers(&self.image.attributes)
    }

    pub fn non_parallel(self) -> Self { Self { parallel: false, ..self } }
    pub fn skip_compatibility_checks(self) -> Self { Self { check_compatibility: false, ..self } }
    pub fn on_progress(self, on_progress: F) -> Self where F: FnMut(f64) { Self { on_progress, ..self } }

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
        let layers = self.image.layer_data.create_writer(&meta_data);

        crate::block::write_all_blocks_to_buffered(
            write, meta_data,
            move |meta, block| layers.extract_uncompressed_block(meta, block),
            self.on_progress, self.check_compatibility, self.parallel,
        )
    }
}

