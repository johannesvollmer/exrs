use crate::meta::header::Header;
use crate::error::Result;
use crate::image::read::{ReadImage, ImageReader};

// TODO filter headers, validate meta_data,



// TODO: this is not beautiful!
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadPedantic<I> { pub reader: I }

impl<'s, I: ReadImage<'s>> ReadImage<'s> for ReadPedantic<I> {
    type Image = <I::Reader as ImageReader>::Image;
    type Reader = I::Reader;
    fn create_image_reader(&'s self, headers: &[Header]) -> Result<Self::Reader> { self.reader.create_image_reader(headers) }
    fn is_sequential(&self) -> bool { self.reader.is_sequential() }
    fn is_pedantic(&self) -> bool { true }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadNonParallel<I> { pub reader: I }
impl<'s, I: ReadImage<'s>> ReadImage<'s> for ReadNonParallel<I> {
    type Image = <I::Reader as ImageReader>::Image;
    type Reader = I::Reader;
    fn create_image_reader(&'s self, headers: &[Header]) -> Result<Self::Reader> { self.reader.create_image_reader(headers) }

    fn is_sequential(&self) -> bool {
        true
    }

    fn is_pedantic(&self) -> bool { self.reader.is_pedantic() }
}


/*pub struct ReadOnProgress<F, I> {
    pub on_progress: F,
    pub read_image: I,
}*/

/*pub struct OnProgressReader<I> {
    total_blocks: usize,
    current_block: usize,
    // on_progress: F,
    image_reader: I,
}*/

// Note: Progress cannot be tracked inside `read_block` because reading is immutable.
//       The progress should be a separate top-level call and not implicitly hidden in the block process.
/*impl<'s, F: 's + FnMut(f64), I: 's> ReadImage<'s> for ReadOnProgress<F, I> where I: ReadImage<'s> {
    type Reader = OnProgressReader<F, I::Reader>;

    fn create_image_reader(&'s self, headers: &[Header]) -> Result<Self::Reader> {
        Ok(OnProgressReader {
            total_blocks: headers.iter().map(|header| header.chunk_count).sum(), // TODO pass block count??? account for filtering??
            current_block: 0,
            // on_progress: &self.on_progress,
            image_reader: self.read_image.create_image_reader(headers)?
        })
    }

    fn is_sequential(&self) -> bool { self.read_image.is_sequential() }
    fn is_pedantic(&self) -> bool { self.read_image.is_pedantic() }
}*/

/*impl<F: FnMut(f64), I: ImageReader> ImageReader for OnProgressReader<I> {
    type Image = I::Image;

    fn filter_block(&self, header: (usize, &Header), tile: (usize, &TileCoordinates)) -> bool {
        self.image_reader.filter_block(header, tile)
    }

    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult {
        let on_progress = &mut self.on_progress;
        on_progress(self.current_block as f64 / self.total_blocks as f64);

        self.current_block += 1;

        self.image_reader.read_block(headers, block)
    }

    fn into_image(self) -> Self::Image {
        debug_assert_eq!(self.current_block, self.total_blocks, "not all blocks have been processed");
        self.image_reader.into_image()
    }
}*/
