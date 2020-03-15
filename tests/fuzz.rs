//! Fuzzy testing.
//! Tries to discover panics with random bytes.
//! This test is expensive and therefore marked with `#[ignore]`. To run this test, use `cargo test -- --ignored`.

use std::panic::{catch_unwind};
use rand::{Rng};
use rand::rngs::{StdRng};

extern crate exr;
use exr::prelude::*;
use std::path::PathBuf;
use std::ffi::OsStr;

fn exr_files(path: &'static str, filter: bool) -> impl Iterator<Item=PathBuf> {
    walkdir::WalkDir::new(path).into_iter().map(std::result::Result::unwrap)
        .filter(|entry| entry.path().is_file())

        .filter(move |entry| !filter || entry.path().extension() == Some(OsStr::new("exr")))
        .map(walkdir::DirEntry::into_path)
}


#[test]
pub fn damaged(){
    let mut passed = true;

    for file in exr_files("tests/images", false) {
        let file = &file;

        let result = catch_unwind(move || {
            exr::image::full::Image::read_from_file(file, read_options::high())
                .and_then(|_| exr::image::simple::Image::read_from_file(file, read_options::high()))
                .and_then(|_| exr::image::rgba::Image::read_from_file(file, read_options::high()))
        });

        // this should not panic, only err:
        passed = passed && match result {
            Ok(Err(Error::Invalid(_))) => {
                println!("✓ No Panic: {:?}", file);
                true
            },
            Ok(Err(Error::NotSupported(_))) => {
                println!("✓ No Panic: {:?}", file);
                true
            },

            Ok(Err(Error::Io(error))) => {
                println!("✗ Unexpected IO Error: {:?}, {:?}", file, error);
                false
            },
            Err(_) => {
                println!("✗ Panic: {:?}", file);
                false
            },

            Ok(_) => true, // ignore ok images
        };
    }

    assert!(passed, "A damaged file was not handled correctly");
}

#[test]
#[ignore]
pub fn fuzz(){
    println!("started fuzzing");
    let files: Vec<PathBuf> = exr_files("tests/images", true).collect();

    let seed = [92,1,0,30,2,8,21,70,74,4,9,9,0,23,0,3,20,5,6,5,9,30,0,34,8,0,40,7,5,02,7,0,];
    let mut random: StdRng = rand::SeedableRng::from_seed(seed);

    let start_index = 0; // default is 0. increase this integer for debugging a specific fuzz case
    for fuzz_index in 0 .. 1024_u64 * 2048 * 4 {

        let file_1_name = &files[random.gen_range(0, files.len())];
        let mutation_point = random.gen::<f32>().powi(12);
        let mutation = random.gen::<u8>();

        if fuzz_index >= start_index {
            let mut file = std::fs::read(file_1_name).unwrap();
            let index = ((mutation_point * file.len() as f32) as usize + 4) % file.len();
            file[index] = mutation;

            let result = catch_unwind(move || {
                match exr::image::full::Image::read_from_buffered(file.as_slice(), read_options::low()) {
                    Err(Error::Invalid(error)) => println!("    [{}]: invalid. {}.", fuzz_index, error),
                    _ => {},
                }
            });

            if let Err(error) = result {
                println!("!!! [{}]: {:?}", fuzz_index, error);
            }
        }

    }
}
