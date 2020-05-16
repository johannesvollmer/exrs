
//! Read and write an exr image.
//! Use `exr::image::simple` or `exr::image::full` for actually reading a complete image.

pub mod full;
pub mod simple;
pub mod rgba;

use crate::error::{UnitResult};


/// Specify how to write an exr image.
#[derive(Debug)]
pub struct WriteOptions<P: OnWriteProgress> {

    /// Enable multi-core compression.
    pub parallel_compression: bool,

    /// If enabled, writing an image throws errors
    /// for files that may look invalid to other exr readers.
    /// Should always be true. Only set this to false
    /// if you can risk never opening the file with another exr reader again,
    /// __ever__, really.
    pub pedantic: bool,

    /// Called occasionally while writing a file.
    /// The first argument is the progress, a float from 0 to 1.
    /// The second argument contains the total number of bytes written.
    /// May return `Error::Abort` to cancel writing the file.
    /// Can be a closure accepting a float and a usize, see `OnWriteProgress`.
    pub on_progress: P,
}

/// Specify how to read an exr image.
#[derive(Debug)]
pub struct ReadOptions<P: OnReadProgress> {

    /// Enable multi-core decompression.
    pub parallel_decompression: bool,

    /// Called occasionally while reading a file.
    /// The argument is the progress, a float from 0 to 1.
    /// May return `Error::Abort` to cancel reading the file.
    /// Can be a closure accepting a float, see `OnReadProgress`.
    pub on_progress: P,

    /// Reading an image is aborted if the memory required for the pixels is too large.
    /// The default value of 1GB avoids reading invalid files.
    pub max_pixel_bytes: Option<usize>,

    /// If true, single invalid attributes do not abort the whole reading process.
    /// If false, reading the file is stopped when an attribute appears to be invalid.
    pub skip_invalid_attributes: bool,

    /// If true, some files will be rejected that are readable but have unconventional properties,
    /// such as two attributes with the same name, or two headers with the same name,
    /// invalid attributes will abort the process.
    /// If false, this will ready any technically valid image file. Invalid attributes will be skipped.
    pub pedantic: bool,
}


/// A collection of preset `WriteOptions` values.
pub mod write_options {
    use super::*;

    /// High speed but also slightly higher memory requirements.
    pub fn default() -> WriteOptions<()> { self::high() }

    /// High speed but also slightly higher memory requirements.
    pub fn high() -> WriteOptions<()> {
        WriteOptions {
            parallel_compression: true, pedantic: true,
            on_progress: (),
        }
    }

    /// Lower speed but also lower memory requirements.
    pub fn low() -> WriteOptions<()> {
        WriteOptions {
            parallel_compression: false, pedantic: true,
            on_progress: (),
        }
    }
}

/// A collection of preset `ReadOptions` values.
pub mod read_options {
    use super::*;

    const GIGABYTE: usize = 1_000_000_000;


    /// High speed but also slightly higher memory requirements.
    /// Skips invalid attributes instead of aborting the reading process.
    pub fn default() -> ReadOptions<()> { self::high() }

    /// High speed but also slightly higher memory requirements.
    /// Aborts reading images that would require more than 1GB of memory.
    /// Skips invalid attributes instead of aborting the reading process.
    pub fn high() -> ReadOptions<()> {
        ReadOptions {
            parallel_decompression: true,
            max_pixel_bytes: Some(GIGABYTE),
            skip_invalid_attributes: true,
            on_progress: (),
            pedantic: false,
        }
    }

    /// Lower speed but also lower memory requirements.
    /// Aborts reading images that would require more than 1GB of memory.
    /// Skips invalid attributes instead of aborting the reading process.
    pub fn low() -> ReadOptions<()> {
        ReadOptions {
            parallel_decompression: false,
            max_pixel_bytes: Some(GIGABYTE),
            skip_invalid_attributes: true,
            on_progress: (),
            pedantic: false,
        }
    }
}


/// Called occasionally when writing a file.
/// Implemented by any closure that matches `|progress: f32, bytes_written: usize| -> UnitResult`.
pub trait OnWriteProgress {

    /// The progress is a float from 0 to 1.
    /// May return `Error::Abort` to cancel writing the file.
    #[must_use]
    fn on_write_progressed(&mut self, progress: f32, bytes_written: usize) -> UnitResult;
}

/// Called occasionally when reading a file.
/// Implemented by any closure that matches `|progress: f32| -> UnitResult`.
pub trait OnReadProgress {

    /// The progress is a float from 0 to 1.
    /// May return `Error::Abort` to cancel reading the file.
    #[must_use]
    fn on_read_progressed(&mut self, progress: f32) -> UnitResult;
}

impl<F> OnWriteProgress for F where F: FnMut(f32, usize) -> UnitResult {
    #[inline] fn on_write_progressed(&mut self, progress: f32, bytes_written: usize) -> UnitResult { self(progress, bytes_written) }
}

impl<F> OnReadProgress for F where F: FnMut(f32) -> UnitResult {
    #[inline] fn on_read_progressed(&mut self, progress: f32) -> UnitResult { self(progress) }
}

impl OnWriteProgress for () {
    #[inline] fn on_write_progressed(&mut self, _progress: f32, _bytes_written: usize) -> UnitResult { Ok(()) }
}

impl OnReadProgress for () {
    #[inline] fn on_read_progressed(&mut self, _progress: f32) -> UnitResult { Ok(()) }
}


