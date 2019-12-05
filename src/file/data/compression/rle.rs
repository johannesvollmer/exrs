use super::*;
use super::optimize_bytes::*;
use super::Error;
use super::Result;

// inspired by  https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfRle.cpp

const MIN_RUN_LENGTH : usize = 3;
const MAX_RUN_LENGTH : usize = 127;

fn take_1(slice: &mut &[u8]) -> Result<u8> {
    if !slice.is_empty() {
        let result = slice[0];
        *slice = &slice[1..];
        Ok(result)

    } else {
        Err(Error::InvalidData)
    }
}

fn take_n<'s>(slice: &mut &'s [u8], n: usize) -> Result<&'s [u8]> {
    if n <= slice.len() {
        let (front, back) = slice.split_at(n);
        *slice = back;
        Ok(front)

    } else {
        Err(Error::InvalidData)
    }
}

pub fn decompress_bytes(compressed: ByteVec, expected_byte_size: usize) -> Result<ByteVec> {
    let mut decompressed = Vec::with_capacity(expected_byte_size);
    let mut remaining = &compressed[..];

    while !remaining.is_empty() {
        let count = take_1(&mut remaining)? as i8 as i32;

        if count < 0 {
            // take the next '-count' bytes as-is
            let values = take_n(&mut remaining, (-count) as usize)?;
            decompressed.extend_from_slice(values);

        } else {
            // repeat the next value 'count' times
            let value = take_1(&mut remaining)?;
            for _ in 0..count + 1 { // TODO memset?
                decompressed.push(value);
            }
        }
    }

    differences_to_samples(&mut decompressed);
    decompressed = interleave_byte_blocks(&decompressed);
    Ok(decompressed)
}

// TODO use BytesRef = &[u8] instead of Bytes!
pub fn compress_bytes(packed: Bytes) -> Result<ByteVec> {
    let mut data = separate_bytes_fragments(&packed);
    samples_to_differences(&mut data);

    // signed char *outWrite = out;
    // const char *runStart = in;
    // const char *runEnd = in + 1;
    // const char *inEnd = in + inLength;
    let mut compressed = Vec::with_capacity(data.len());
    let mut run_start = 0;
    let mut run_end = 1;


    // while (runStart < inEnd) {
    while run_start < data.len() {
        // while (runEnd < inEnd && *runStart == *runEnd && runEnd - runStart - 1 < MAX_RUN_LENGTH) {
        //     ++runEnd;
        // }
        while
            run_end < data.len()
                && data[run_start] == data[run_end]
                && (run_end - run_start) as i32 - 1 < MAX_RUN_LENGTH as i32
            {
                run_end += 1;
            }

        // if (runEnd - runStart >= MIN_RUN_LENGTH) {
        if run_end - run_start >= MIN_RUN_LENGTH {
            // *outWrite++ = (runEnd - runStart) - 1;
            // *outWrite++ = *(signed char *) runStart;
            // runStart = runEnd;
            compressed.push(((run_end - run_start) as i32 - 1) as u8);
            compressed.push(data[run_start]);
            run_start = run_end;

        } else {
            //    while (
            //          runEnd < inEnd
            //          && (
            //                 (runEnd + 1 >= inEnd || *runEnd != *(runEnd + 1))
            //              || (runEnd + 2 >= inEnd || *(runEnd + 1) != *(runEnd + 2))
            //          )
            //          && runEnd - runStart < MAX_RUN_LENGTH)
            //    {
            //        ++runEnd;
            //    }
            while
                run_end < data.len() && (
                    (run_end + 1 >= data.len() || data[run_end] != data[run_end + 1])
                        || (run_end + 2 >= data.len() || data[run_end + 1] != data[run_end + 2])
                ) && run_end - run_start < MAX_RUN_LENGTH
                {
                    run_end += 1;
                }

            // *outWrite++ = runStart - runEnd;
            compressed.push((run_start as i32 - run_end as i32) as u8);

            // TODO use memcpy?
            //    while (runStart < runEnd) {
            //        *outWrite++ = *(signed char *) (runStart++);
            //    }
            while run_start < run_end {
                compressed.push(data[run_start]);
                run_start += 1;
            }

            // ++runEnd;
            run_end += 1;
        }
    }

    Ok(compressed)
}
