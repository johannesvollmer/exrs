use super::*;
use super::optimize_bytes::*;

// taken from https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfRle.cpp

const MIN_RUN_LENGTH : usize = 3;
const MAX_RUN_LENGTH : usize = 127;

/// panics on empty slice
// TODO return Err to avoid panic on invalid data
fn take_1(slice: &mut &[u8]) -> u8 {
    let result = slice[0];
    *slice = &slice[1..];
    result
}

/// panics on empty slice
// TODO return Err to avoid panic on invalid data
fn take_n<'s>(slice: &mut &'s [u8], n: usize) -> &'s [u8] {
    let (front, back) = slice.split_at(n);
    *slice = back;
    front
//    let result = &slice[0..n];
//    *slice = &slice[n..];
//    result
}

pub fn decompress_bytes(target: UncompressedData, compressed: &CompressedData, line_size: usize) -> Result<UncompressedData> {
    /*let max_bytes_per_line = match target {
        DataBlock::ScanLine(ref channels) => channels.len() * line_size * 4,
        DataBlock::Tile(ref channels) => channels.len() * line_size * 4,
        _ => panic!()
    } as i32;*/

    let mut decompressed = Vec::with_capacity(3 * (compressed.len() / 2));
    let mut remaining = &compressed[..];

    while !remaining.is_empty() {
        let count = take_1(&mut remaining) as i8;

        println!("count: {}", count); // FIXME count should not be 2, because min encoding length is 3?

        if count < 0 {
            // an uncompressed run of values is preceded by a negative count
            let values = take_n(&mut remaining, (-count) as usize);
            decompressed.extend_from_slice(values);

        } else {
            // a repeated value is preceded by a positive count
            let value = take_1(&mut remaining);
            for _ in 0..count { // TODO memset?
                decompressed.push(value);
            }
        }
    }

    /*
    char *outStart = out;
    while (inLength > 0){
        if (*in < 0){
            int count = -((int)*in++);
            inLength -= count + 1;

            if (0 > (maxLength -= count))
                return 0;

            memcpy(out, in, count);
            out += count;
            in  += count;
        } else {
            int count = *in++;
            inLength -= 2;

            if (0 > (maxLength -= count + 1))
                return 0;

            memset(out, *(char*)in, count+1);
            out += count+1;

            in++;
        }
    }

    return out - outStart;
    */



    /*let mut index = 0 as i32;
    let mut remaining = compressed.len() as i32; // TODO use in_end instead?

    while remaining > 0 {
        if (compressed[index as usize] as i8) < 0 {
            let count = - (compressed[index as usize] as i8 as i32);
            index += 1;
            remaining -= count + 1;

            decompressed.extend_from_slice(&compressed[index as usize .. (index + count) as usize]);
            index += count;

        } else {
            let count = compressed[index as usize] as i8 as i32;
            index += 1;
            remaining -= 2;

            decompressed.extend_from_slice(&compressed[index as usize .. (index + count + 1) as usize]);
            index += 1;
        }
    }*/




    differences_to_samples(&mut decompressed); // TODO per channel? per line??
    decompressed = interleave_byte_blocks(&decompressed);
    super::uncompressed::unpack(target, &decompressed, line_size) // convert to machine-dependent endianess
}

pub fn compress_bytes(data: &UncompressedData) -> Result<CompressedData> {
    let mut data = super::uncompressed::pack(data)?; // convert from machine-dependent endianess
    data = separate_bytes_fragments(&data);
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
