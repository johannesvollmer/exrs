extern crate smallvec;

use std::{panic};
use std::path::{PathBuf};
use rayon::prelude::{ParallelBridge, ParallelIterator};

/// Check whether git-lfs large files are properly initialized
// FIXME might only work if there is at least one text file in the directory, never on binaries?
#[test]
fn check_large_files(){
    let valid = walkdir::WalkDir::new("tests/images").into_iter()
        .map(std::result::Result::unwrap).map(walkdir::DirEntry::into_path)
        .par_bridge().all(|file: PathBuf| {

            // check if this file contains multiple keywords
            std::fs::read_to_string(file)
                .map(|file|{
                    !(
                        file.contains("git-lfs")
                        && file.contains("version ")
                        && file.contains("oid ")
                    )
                }).unwrap_or(true) // invalid UTF-8 is probably a valid binary file
        });

    assert!(valid, "'Git LFS' Large Files are not properly initialized!");
}
