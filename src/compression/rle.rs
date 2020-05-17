use super::*;
use super::optimize_bytes::*;
use super::Error;
use super::Result;

// inspired by  https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfRle.cpp

const MIN_RUN_LENGTH : usize = 3;
const MAX_RUN_LENGTH : usize = 127;


pub fn decompress_bytes(mut remaining: Bytes<'_>, expected_byte_size: usize) -> Result<ByteVec> {
    let mut decompressed = Vec::with_capacity(expected_byte_size.min(8*2048));

    while !remaining.is_empty() {
        let count = take_1(&mut remaining)? as i8 as i32;

        if count < 0 {
            // take the next '-count' bytes as-is
            let values = take_n(&mut remaining, (-count) as usize)?;
            decompressed.extend_from_slice(values);
        }
        else {
            // repeat the next value 'count + 1' times
            let value = take_1(&mut remaining)?;
            decompressed.resize(decompressed.len() + count as usize + 1, value);
        }
    }

    differences_to_samples(&mut decompressed);
    interleave_byte_blocks(&mut decompressed);
    Ok(decompressed)
}

pub fn compress_bytes(data: Bytes<'_>) -> Result<ByteVec> {
    let mut data = Vec::from(data); // TODO no alloc
    separate_bytes_fragments(&mut data);
    samples_to_differences(&mut data);

    let mut compressed = Vec::with_capacity(data.len());
    let mut run_start = 0;
    let mut run_end = 1;

    while run_start < data.len() {
        while
            run_end < data.len()
                && data[run_start] == data[run_end]
                && (run_end - run_start) as i32 - 1 < MAX_RUN_LENGTH as i32
            {
                run_end += 1;
            }

        if run_end - run_start >= MIN_RUN_LENGTH {
            compressed.push(((run_end - run_start) as i32 - 1) as u8);
            compressed.push(data[run_start]);
            run_start = run_end;

        } else {
            while
                run_end < data.len() && (
                    (run_end + 1 >= data.len() || data[run_end] != data[run_end + 1])
                        || (run_end + 2 >= data.len() || data[run_end + 1] != data[run_end + 2])
                ) && run_end - run_start < MAX_RUN_LENGTH
                {
                    run_end += 1;
                }

            compressed.push((run_start as i32 - run_end as i32) as u8);
            compressed.extend_from_slice(&data[run_start .. run_end]);

            run_start = run_end;
            run_end += 1;
        }
    }

    Ok(compressed)
}

fn take_1(slice: &mut &[u8]) -> Result<u8> {
    if !slice.is_empty() {
        let result = slice[0];
        *slice = &slice[1..];
        Ok(result)

    } else {
        Err(Error::invalid("compressed data"))
    }
}

fn take_n<'s>(slice: &mut &'s [u8], n: usize) -> Result<&'s [u8]> {
    if n <= slice.len() {
        let (front, back) = slice.split_at(n);
        *slice = back;
        Ok(front)

    } else {
        Err(Error::invalid("compressed data"))
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn test(){
        let data = vec![ 0, 23, 4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 0, 0, 0, 1, 23, 43, 4];
        let compressed = super::compress_bytes(&data).unwrap();
        let decompressed = super::decompress_bytes(&compressed, data.len()).unwrap();

        assert_eq!(decompressed, data);
    }

    // TODO fuzz testing
}