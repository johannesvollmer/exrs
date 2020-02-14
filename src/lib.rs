

#![warn(
    rust_2018_idioms,
    future_incompatible,
    unused_extern_crates,
    unused,

    missing_copy_implementations,
    missing_debug_implementations,

    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
)]

#![deny(
    unused_variables,
    unused_assignments,
    dead_code,
    unused_must_use,
    missing_copy_implementations,
    trivial_numeric_casts,
    redundant_semicolon
)]

#![forbid(unsafe_code)]

// TODO #![warn(missing_docs)]


pub mod io; // public to allow for custom attribute byte parsing

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

/// Re-exports of all modules types commonly required for simple reading and writing of an exr image.
pub mod prelude {
    // main exports
    pub use crate::meta::MetaData;

    pub use crate::image::{simple};

    // secondary data types
    pub use crate::meta;
    pub use crate::meta::attributes;
    pub use crate::error;

    // re-export external stuff
    pub use half::f16;

    // export real types and attributes
    pub use crate::math::Vec2;
    pub use attributes::{ Compression, Text, IntRect, };
    pub use error::{ Result, Error };
}



