

//! Read and write OpenEXR images.
//! This library uses no foreign code or unsafe Rust.
//!
//! See the [README.md](https://github.com/johannesvollmer/exrs/blob/master/README.md) for more information,
//! or check out the [examples](https://github.com/johannesvollmer/exrs/tree/master/examples).

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
#![warn(missing_docs)]

/// Print the name and value of each variable.
#[macro_export]
macro_rules! debug {
    ( $( $var: expr ),* ) => {
        {
            $(
                println!("{} = {:?}", stringify!($var), $var);
            )*
        }
    };

    ($name: expr, $val: expr) => {
        {
            println!("{} = {:?}", name, $val);
            $val
        }
    };
}

pub mod io; // public to allow for custom attribute byte parsing

pub mod math;
pub mod chunk;
pub mod compression;
pub mod meta;
pub mod image;

pub mod error;
pub mod block;

#[macro_use]
extern crate smallvec;

#[allow(unused)] // this is a dev dependency
#[cfg(test)]
extern crate image as piston_image;

/// Re-exports of all modules types commonly required for simple reading and writing of an exr image.
pub mod prelude {

    pub use crate::meta::{ self, attributes, LayerAttributes, ImageAttributes };
    pub use self::attributes::{ Compression, Text, IntRect, LineOrder, SampleType, TileDescription };
    pub use crate::error::{ Result, Error };
    pub use crate::math::Vec2;

    pub use crate::image::{
        simple, rgba,
        write_options, read_options,
        WriteOptions, ReadOptions
    };


    // re-export external stuff
    pub use half::f16;

}



