
//! Wavelet encoding and decoding.
// see https://github.com/AcademySoftwareFoundation/openexr/blob/8cd1b9210855fa4f6923c1b94df8a86166be19b1/OpenEXR/IlmImf/ImfWav.cpp

use crate::error::IoResult;
use crate::math::Vec2;

pub fn encode(
    buffer: &mut [u16],
    Vec2(count_x, count_y): Vec2<usize>,
    Vec2(offset_x, offset_y): Vec2<usize>,
    max: u16 //  maximum buffer[i] value
) -> IoResult<()>
{
    let is_14_bit = max < (1 << 14);
    let count = count_x.min(count_y);
    let mut p: usize = 1; // TODO i32?
    let mut p2: usize = 2;

    while p2 <= count {

        let mut position_y = 0;
        let end_y = 0 + offset_y * (count_y - p2);
        let (offset1_x, offset1_y) = (offset_x * p, offset_y * p);
        let (offset2_x, offset2_y) = (offset_x * p2, offset_y * p2);

        // y-loop
        while position_y <= end_y { // TODO: for py in (index..ey).nth(offset_2.0)

            let mut position_x = position_y;
            let end_x = position_x + offset_x * (count_x - p2);

            // x-loop
            while position_x <= end_x {
                let pos_right = position_x + offset1_x;
                let pos_top = position_x + offset1_y;
                let pos_top_right = pos_top + offset1_x;

                assert!(position_x < buffer.len());
                assert!(pos_right < buffer.len());
                assert!(pos_top < buffer.len());
                assert!(pos_top_right < buffer.len());

                let encode = if is_14_bit {
                    debug_assert!(buffer[position_x] < (1 << 14));
                    debug_assert!(buffer[pos_right] < (1 << 14));

                    encode_14bit
                }
                else {
                    encode_16bit
                };

                let (center, right) = encode(buffer[position_x], buffer[pos_right]);
                let (top, top_right) = encode(buffer[pos_top], buffer[pos_top_right]);

                let (center, top) = encode(center, top);
                let (right, top_right) = encode(right, top_right);

                buffer[position_x] = center; // TODO rustify
                buffer[pos_top] = top;
                buffer[pos_right] = right;
                buffer[pos_top_right] = top_right;

                position_x += offset2_x;
            }

            // encode remaining odd pixel column
            if count_x & p != 0 {
                let pos_top = position_x + offset1_y;
                let (center, top) = {
                    if is_14_bit { encode_14bit(buffer[position_x], buffer[pos_top]) }
                    else { encode_16bit(buffer[position_x], buffer[pos_top]) }
                };

                buffer[position_x] = center;
                buffer[pos_top] = top;
            }

            position_y += offset2_y;
        }

        // encode possibly remaining odd row
        if count_y & p != 0 {
            let mut position_x = position_y;
            let end_x = position_y + offset_x * (count_x - p2);

            while position_x <= end_x {
                let pos_right = position_x + offset1_x;

                let (center, right) = {
                    if is_14_bit { encode_14bit(buffer[position_x], buffer[pos_right]) }
                    else { encode_16bit(buffer[position_x], buffer[pos_right]) }
                };

                buffer[pos_right] = right;
                buffer[position_x] = center;

                position_x += offset2_x;
            }
        }

        p = p2;
        p2 <<= 1;
    }

    Ok(())
}


pub fn decode(
    buffer: &mut [u16],
    Vec2(count_x, count_y): Vec2<usize>,
    Vec2(offset_x, offset_y): Vec2<usize>,
    max: u16 //  maximum buffer[i] value
) -> IoResult<()>
{
    let is_14_bit = max < (1 << 14);
    let count = count_x.min(count_y);
    let mut p: usize = 1; // TODO i32?
    let mut p2: usize; // TODO i32?

    // search max level
    while p <= count {
        p <<= 1;
    }

    p >>= 1;
    p2 = p;
    p >>= 1;

    while p >= 1 {

        let mut position_y = 0;
        let end_y = 0 + offset_y * (count_y - p2);

        let (offset1_x, offset1_y) = (offset_x * p, offset_y * p);
        let (offset2_x, offset2_y) = (offset_x * p2, offset_y * p2);

        debug_assert_ne!(offset_x, 0, "offset is zero (but shouldnt be???)"); // ????
        debug_assert_ne!(offset_y, 0, "offset is zero (but shouldnt be???)"); // ????

        while position_y <= end_y {
            let mut position_x = position_y;
            let end_x = position_x + offset_x * (count_x - p2);

            while position_x <= end_x {
                let pos_right = position_x + offset1_x;
                let pos_top = position_x + offset1_y;
                let pos_top_right = pos_top + offset1_x;

                assert!(position_x < buffer.len());
                assert!(pos_right < buffer.len());
                assert!(pos_top < buffer.len());
                assert!(pos_top_right < buffer.len());

                let decode = if is_14_bit { decode_14bit } else { decode_16bit };

                let (center, top) = decode(buffer[position_x], buffer[pos_top]);
                let (right, top_right) = decode(buffer[pos_right], buffer[pos_top_right]);

                let (center, right) = decode(center, right);
                let (top, top_right) = decode(top, top_right);

                buffer[position_x] = center; // TODO rustify
                buffer[pos_top] = top;
                buffer[pos_right] = right;
                buffer[pos_top_right] = top_right;

                position_x += offset2_x;
            }

            // decode last odd remaining x value
            if count_x & p != 0 {
                let pos_top = position_x + offset1_y;
                let (center, top) = {
                    if is_14_bit { decode_14bit(buffer[position_x], buffer[pos_top]) }
                    else { decode_16bit(buffer[position_x], buffer[pos_top]) }
                };

                buffer[position_x] = center;
                buffer[pos_top] = top;
            }

            position_y += offset2_y;
        }

        // decode remaining odd row
        if count_y & p != 0 {
            let mut position_x = position_y;
            let end_x = position_x + offset_x * (count_x - p2);

            while position_x <= end_x {
                let pos_right = position_x + offset1_x;

                let (center, right) = {
                    if is_14_bit { decode_14bit(buffer[position_x], buffer[pos_right]) }
                    else { decode_16bit(buffer[position_x], buffer[pos_right]) }
                };

                buffer[position_x] = center;
                buffer[pos_right] = right;

                position_x += offset2_x;
            }
        }

        p2 = p;
        p >>= 1;
    }

    Ok(())
}


/// Untransformed data values should be less than (1 << 14).
#[inline]
fn encode_14bit(a: u16, b: u16) -> (u16, u16) {
    let (a, b) = (a as i16, b as i16);

    let m = (a + b) >> 1;
    let d = a - b;

    (m as u16, d as u16) // TODO explicitly wrap?
}

#[inline]
fn decode_14bit(l: u16, h: u16) -> (u16, u16) {
    let (l, h) = (l as i16, h as i16);

    let hi = h as i32;
    let ai = l as i32 + (hi & 1) + (hi >> 1);

    let a = ai as i16; // TODO explicitly wrap?
    let b = (ai - hi) as i16; // TODO explicitly wrap?

    (a as u16, b as u16) // TODO explicitly wrap?
}


const BIT_COUNT: i32 = 16;
const OFFSET: i32 = 1 << (BIT_COUNT - 1);
const MOD_MASK: i32 = (1 << BIT_COUNT) - 1;

#[inline]
fn encode_16bit(a: u16, b: u16) -> (u16, u16) {
    let (a, b) = (a as i32, b as i32);

    let a_offset = (a + OFFSET) & MOD_MASK;
    let mut m = (a_offset + b) >> 1;
    let d = a_offset - b;

    if d < 0 { m = (m + OFFSET) & MOD_MASK; }
    let d = d & MOD_MASK;

    (m as u16, d as u16) // TODO explicitly wrap?
}

#[inline]
fn decode_16bit(l: u16, h: u16) -> (u16, u16) {
    let (m, d) = (l as i32, h as i32);

    let b = (m - (d >> 1)) & MOD_MASK;
    let a = (d + b - OFFSET) & MOD_MASK;

    (a as u16, b as u16) // TODO explicitly wrap?
}



#[cfg(test)]
mod test {
    use crate::math::Vec2;

    #[test]
    fn roundtrip_14_bit_values(){
        let data = [
            (13, 54), (3, 123), (423, 53), (1, 23), (23, 515), (513, 43),
            (16374, 16381), (16284, 3), (2, 1), (0, 0), (0, 4), (3, 0)
        ];

        for &values in &data {
            let (l, h) = super::encode_14bit(values.0, values.1);
            let result = super::decode_14bit(l, h);
            assert_eq!(values, result);
        }
    }

    #[test]
    fn roundtrip_16_bit_values(){
        let data = [
            (13, 54), (3, 123), (423, 53), (1, 23), (23, 515), (513, 43),
            (16385, 56384), (18384, 36384), (2, 1), (0, 0), (0, 4), (3, 0)
        ];

        for &values in &data {
            let (l, h) = super::encode_16bit(values.0, values.1);
            let result = super::decode_16bit(l, h);
            assert_eq!(values, result);
        }
    }

    #[test]
    fn roundtrip_14bit_image(){
        let data: [u16; 6 * 4] = [
            13, 54, 3, 123, 423, 53,
            1, 23, 23, 515, 513, 43,
            16374, 16381, 16284, 3, 2, 1,
            0, 0, 0, 4, 3, 0,
        ];

        let max = *data.iter().max().unwrap();
        debug_assert!(max < (1 << 14));

        let mut transformed = data.clone();

        super::encode(&mut transformed, Vec2(6, 4), Vec2(1,6), max).unwrap();
        super::decode(&mut transformed, Vec2(6, 4), Vec2(1,6), max).unwrap();

        assert_eq!(data, transformed);
    }

    #[test]
    fn roundtrip_16bit_image(){
        let data: [u16; 6 * 4] = [
            13, 54, 3, 123, 423, 53,
            1, 23, 23, 515, 513, 43,
            16385, 56384, 18384, 36384, 2, 1,
            0, 0, 0, 4, 3, 0,
        ];

        let max = *data.iter().max().unwrap();
        debug_assert!(max >= (1 << 14));

        let mut transformed = data.clone();

        super::encode(&mut transformed, Vec2(6, 4), Vec2(1,6), max).unwrap();
        super::decode(&mut transformed, Vec2(6, 4), Vec2(1,6), max).unwrap();

        assert_eq!(data, transformed);
    }
}