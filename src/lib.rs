
#![forbid(unsafe_code)]

#![deny(
    warnings,

    unused_variables,
    unused_assignments,
    dead_code,
    unused_must_use,
    unused_extern_crates,
    missing_copy_implementations,
    trivial_numeric_casts,
)]

#![warn(
    rust_2018_idioms,
    future_incompatible,
    unused,

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
        ReadOptions, WriteOptions, BlockOptions,
    };

    pub use crate::image::full;

    // secondary data types
    pub use crate::meta;
    pub use crate::meta::attributes;
    pub use crate::error;

    // re-export external stuff
    pub use half::f16;
}



