

/* TODO
use crate::image::write::{WriteImage, ImageWriter};
use crate::meta::header::Header;
use crate::meta::Headers;
use crate::block::BlockIndex;

pub struct WriteImageWithProgress<I, F> {
    pub inner: I, // impl WriteImage
    pub on_progress: F, // impl FnMut(f64)
}

pub struct OnProgressImageWriter<I, F> {
    inner: I, // impl ImageWriter
    on_progress: F, // impl FnMut(f64)
    total_blocks: usize,
    processed_blocks: usize,
}

impl<I, F> WriteImage for WriteImageWithProgress<I, F> where I: WriteImage, F: FnMut(f64) + Sync {
    fn is_pedantic(&self) -> bool { self.inner.is_pedantic() }
    fn is_parallel(&self) -> bool { self.inner.is_parallel() }

    type Writer = OnProgressImageWriter<I::Writer, F>;

    fn infer_meta_data(&self) -> Headers {
        self.inner.infer_meta_data()
    }

    fn create_image_writer(&self, headers: &[Header]) -> Self::Writer {
        OnProgressImageWriter {
            inner: self.inner.create_image_writer(headers),
            on_progress: self.on_progress,
            total_blocks: headers.iter().map(|header| header.chunk_count).sum(), // TODO filtered?
            processed_blocks: 0
        }
    }
}

impl<I, F> ImageWriter for OnProgressImageWriter<I, F> where I: ImageWriter, F: Sync + FnMut(f64) {
    fn extract_uncompressed_block(&self, headers: &[Header], block: BlockIndex) -> Vec<u8> {
        let block = self.inner.extract_uncompressed_block(headers, block);

        self.processed_blocks += 1;
        let function = &mut self.on_progress;
        function(self.processed_blocks as f64 / self.total_blocks as f64);

        block
    }
}*/









/*
pub trait WriteImageWithOptions: Sized {
    fn on_progress<F>(self, on_progress: F) -> WriteOnProgress<F, Self> where F: FnMut(f64);
}

impl<T> WriteWithOptions for T where T: WriteImage {
    fn on_progress<F>(self, on_progress: F) -> WriteOnProgress<F, Self> {
        WriteOnProgress { write: self, on_progress }
    }
}

struct WriteOnProgress<W, F> {
    write: W,
    on_progress: F,
}

impl<W, F> WriteImage for WriteOnProgress<W, F> {
    type Writer = OnProgresWriter<W>;

    fn infer_meta_data(&mut self) -> Headers {
        self.write.infer_meta_data()
    }

    fn create_writer(self, headers: &[Header]) -> Self::Writer {
        OnProgressWriter {
            processed_chunks: 0,
            on_progress: self.on_progress,
            total_chunks: headers.iter().map(|head| head.chunk_count).sum(),
            write: self.write.create_writer(headers)
        }
    }
}




pub struct OnProgressWriter<F, I> {
    processed_chunks: usize,
    total_chunks: usize,
    on_progress: F,
    write: I,
}

impl<F, I> ImageWriter for OnProgressWriter<F, I> where F: FnMut(f64), I: WriteImage {
    fn extract_uncompressed_block(&mut self, headers: &[Header], block: BlockIndex) -> Vec<u8> {
        if self.total_chunks == 0 { self.total_chunks = headers.iter().map(|head| head.chunk_count).sum(); } // TODO not like this??
        let block = self.write.extract_uncompressed_block(headers, block);
        self.on_progress(self.processed_chunks as f64 / self.total_chunks as f64);
        self.processed_chunks += 1;
        block
    }
}


pub struct WriteWithoutValidation<I> {
    write: I,
}

impl<I> WriteImage for WriteWithoutValidation<I> where I: WriteImage {
    fn generate_meta_data(&self) -> Headers { self.write.generate_meta_data() }
    fn extract_uncompressed_block(&self, headers: &[Header], block: BlockIndex) -> Vec<u8>
        { self.write.extract_uncompressed_block(headers, block) }

    fn omit_validation(&self) -> bool {
        true
    }

    fn is_sequential(&self) -> bool { self.write.is_sequential() }
}


pub struct WriteNonParallel<I> {
    write: I,
}

impl<I> WriteImage for WriteNonParallel<I> where I: WriteImage {
    fn generate_meta_data(&self) -> Headers { self.write.generate_meta_data() }
    fn extract_uncompressed_block(&self, headers: &[Header], block: BlockIndex) -> Vec<u8>
        { self.write.extract_uncompressed_block(headers, block) }

    fn omit_validation(&self) -> bool { self.write.omit_validation() }

    fn is_sequential(&self) -> bool {
        true
    }
}*/