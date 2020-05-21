

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
    redundant_semicolons
)]

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Print the name and value of each variable.
#[macro_export]
macro_rules! inspect {
    ( $( $var: expr ),* ) => {
        {
            println!("\nInspecting at {}:{}", file!(), line!());

            $(
                println!("\t{} => {:?}", stringify!($var), $var);
            )*

            print!("\n");
        }
    };

    ($name: expr, $val: expr) => {
        {
            print!("\nInspecting at {}:{} expression {}: ", file!(), line!(), $name);
            println!("{} => {:?}", stringify!($val), $val);
            print!("\n");

            $val
        }
    };
}

pub mod io; // public to allow for custom attribute byte parsing

pub mod math;
pub mod compression;
pub mod meta;
pub mod image;

pub mod error;
pub mod block;

#[macro_use]
extern crate smallvec;


/// Use either `exr::prelude::rgba_image::*` or `exr::prelude::simple_image::*` for simply reading an image.
pub mod prelude {

    /// Re-exports of all common elements needed for reading or writing an `exrs::image::rgba`.
    pub mod rgba_image {
        pub use super::common::*;
        pub use crate::image::rgba::*;
    }

    /// Re-exports of all common elements needed for reading or writing an `exrs::image::simple`.
    pub mod simple_image {
        pub use super::common::*;
        pub use crate::image::simple::*;
    }

    // TODO
    #[doc(hidden)]
    mod full {
        pub use super::common::*;
        pub use crate::image::full::*;
    }

    /// Exports of all modules types commonly required for reading and writing of an exr image.
    /// Use either `exr::prelude::rgba_image::*` or `exr::prelude::simple_image::*` for reading an image.
    /// _Note: This includes a type called `Result`, possibly overwriting the default `std::Result` type usage._
    pub mod common {
        pub use crate::meta::{self, MetaData, attribute, header::{LayerAttributes, ImageAttributes } };
        pub use crate::meta::attribute::{ AttributeValue, Compression, Text, IntRect, LineOrder, SampleType, TileDescription };
        pub use crate::error::{ Result, Error };
        pub use crate::math::Vec2;

        pub use crate::image::{
            write_options, read_options,
            WriteOptions, ReadOptions
        };

        // re-export external stuff
        pub use half::f16;
        pub use smallvec::SmallVec;
        pub use std::convert::TryInto;
    }
}



