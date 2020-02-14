use std::panic::{catch_unwind, resume_unwind};
use std::panic;
use rayon::prelude::*;
use rand::{thread_rng, Rng, SeedableRng};
use rand::rngs::StdRng;
use std::io::Read;

extern crate exr;

struct RandomReader {
    generator: StdRng,
    count: usize,

    result: Vec<u8>,
}

impl RandomReader {
    pub fn new(index: u64) -> Self {
        let mut seed = [0_u8; 32];
        for slice in seed.chunks_exact_mut(8) {
            slice.copy_from_slice(&index.to_le_bytes());
        }

        let mut generator: StdRng = rand::SeedableRng::from_seed(seed);
        Self { count: generator.gen_range(0, 2048*16), generator, result: Vec::new() }
    }
}

impl Read for RandomReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        for (index, byte) in buf.iter_mut().enumerate() {
            if self.count == 0 {
                return Ok(index);
            }

            *byte = self.generator.gen();
            self.result.push(*byte);
            self.count -= 1;
        }

        Ok(buf.len())
    }
}


#[test]
pub fn incremental(){
    println!("started incremental fuzzing");
//    panic::set_hook(Box::new(|_| (/* do not println panics */)));
    let mut pool = rayon::ThreadPoolBuilder::new().build().unwrap();

    for len in 0 .. 32 {
        println!("starting fuzzy testing for byte length of {}", len);

        for variation_index in 0 .. 256_u64.pow(len) {
            pool.install(|| {
                let mut bytes = vec![0_u8; len as usize]; // TODO generate lazily instead of vectored??

                for index in 0..len {
                    let base = len - index - 1;
                    let range = 256_u64.pow(base);

                    bytes[index as usize] = (variation_index / range) as u8;
                }

                if catch_unwind(|| test_bytes(bytes.as_slice())).is_err() {
                    println!("found panics at variation index {}", variation_index);
                }
            })
        }
    }
}


#[test]
pub fn stochastic(){
    println!("started stochastic fuzzing");
    let mut pool = rayon::ThreadPoolBuilder::new().build().unwrap();

    for index in 0..1024_u64 * 2048 * 4 {
        pool.install(move || {
            let mut reader = RandomReader::new(index);

            if catch_unwind(move || {

                // TODO this always already fails at magic number
                let result = test_bytes(reader.by_ref());

                if result.is_ok() {
                    println!("found valid image at byte sequence {:?}", reader.result);
                }

                println!("tested byte sequence {:?}, found {:?}", reader.result, result);

            }).is_err() {
                println!("found panics at index {:?}", index);
            }
        })
    }
}

// should not panic
pub fn test_bytes(bytes: impl Read + Send) -> exr::error::Result<exr::image::full::Image> {
    bencher::black_box(exr::image::full::Image::read_from_buffered(
        bytes, exr::image::full::ReadOptions::debug()
    ))
}