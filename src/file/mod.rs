
//! The `file` module represents the file how it is laid out in memory.


pub mod io;
pub mod meta;
pub mod data;







/*
/// This is the raw data of the file,
/// which can be obtained from a byte stream with minimal processing overhead
/// or written to a byte stream with minimal processing overhead.
///
/// It closely resembles the actual file layout and supports all openEXR features natively.
/// Converting this from or to a boring RGBA array requires more processing and loses information,
/// which is thus optional
#[derive(Debug, Clone)]
pub struct File {
    pub meta_data: meta::MetaData,
    pub chunks: data::compressed::Chunks,
}
*/





pub mod validity {
    // TODO put validation into own module
    pub type Validity = Result<(), Invalid>;

    #[derive(Debug, Clone, Copy)]
    pub enum Invalid {
        Missing(Value),
        NotSupported(&'static str),
        Combination(&'static [Value]),
        Content(Value, Required),
        Type(Required),
    }

    #[derive(Debug, Clone, Copy)]
    pub enum Value {
        Attribute(&'static str),
        Version(&'static str),
        Chunk(&'static str),
        Type(&'static str),
        Part(&'static str),
        Enum(&'static str),
        Text,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum Required {
        Max(usize),
        Min(usize),
        Exact(&'static str),
        OneOf(&'static [&'static str]),
        Range {
            /// inclusive
            min: usize,

            /// inclusive
            max: usize
        },
    }
}




