
#![forbid(unsafe_code)]
#![forbid(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
)]

// TODO #![warn(missing_docs)]


pub mod io;
pub mod math;
pub mod chunks;
pub mod compression;
pub mod meta;
pub mod image;
pub mod error;

#[macro_use]
extern crate smallvec;

#[cfg(test)]
extern crate image as piston_image;


pub mod prelude {
    // main exports
    pub use crate::meta::MetaData;

    // core data types
    pub use crate::image::{
        read_from_file, write_to_file,
        ReadOptions, WriteOptions, TileOptions,
    };

    pub use crate::image::full;

    // secondary data types
    pub use crate::meta;
    pub use crate::meta::attributes;
    pub use crate::error;

    // re-export external stuff
    pub use half::f16;
}



