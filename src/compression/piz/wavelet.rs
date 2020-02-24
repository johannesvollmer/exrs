
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
    // bool w14 = (mx < (1 << 14));
    //     int	n  = (nx > ny)? ny: nx;
    //     int	p  = 1;			// == 1 <<  level
    //     int p2 = 2;			// == 1 << (level+1)
    let is_14_bit = max < (1 << 14);
    let count = count_x.min(count_y);
    let mut p: usize = 1; // TODO i32?
    let mut p2: usize = 2;

    //     while (p2 <= n)
    while p2 <= count {

        // 	unsigned short *py = in;
        // 	unsigned short *ey = in + oy * (ny - p2);
        // 	int oy1 = oy * p;
        // 	int oy2 = oy * p2;
        // 	int ox1 = ox * p;
        // 	int ox2 = ox * p2;
        // 	unsigned short i00,i01,i10,i11;
        let mut position_y = 0;
        let end_y = 0 + offset_y * (count_y - p2);
        let (offset1_x, offset1_y) = (offset_x * p, offset_y * p);
        let (offset2_x, offset2_y) = (offset_x * p2, offset_y * p2);

        // 	for (; py <= ey; py += oy2)

        // y-loop
        while position_y <= end_y { // TODO: for py in (index..ey).nth(offset_2.0)

            // 	    unsigned short *px = py;
            // 	    unsigned short *ex = py + ox * (nx - p2);
            let mut position_x = position_y;
            let end_x = position_x + offset_x * (count_x - p2);

            // 	    for (; px <= ex; px += ox2)

            // x-loop
            while position_x <= end_x {
                // 		unsigned short *p01 = px  + ox1;
                // 		unsigned short *p10 = px  + oy1;
                // 		unsigned short *p11 = p10 + ox1;
                let p01 = position_x + offset1_x;
                let p10 = position_x + offset1_y;
                let p11 = p10 + offset1_x;

                assert!(position_x < buffer.len());
                assert!(p01 < buffer.len());
                assert!(p10 < buffer.len());
                assert!(p11 < buffer.len());

                if is_14_bit {
                    // 		    wenc14 (*px,  *p01, i00, i01);
                    // 		    wenc14 (*p10, *p11, i10, i11);
                    // 		    wenc14 (i00, i10, *px,  *p10);
                    // 		    wenc14 (i01, i11, *p01, *p11);

                    debug_assert!(buffer[position_x] < (1 << 14));
                    debug_assert!(buffer[p01] < (1 << 14));

                    let (i00, i01) = encode_14bit(buffer[position_x], buffer[p01]);
                    let (i10, i11) = encode_14bit(buffer[p10], buffer[p11]);

                    let (px_, p10_) = encode_14bit(i00, i10);
                    let (p01_, p11_) = encode_14bit(i01, i11);

                    buffer[position_x] = px_; // TODO rustify
                    buffer[p10] = p10_;
                    buffer[p01] = p01_;
                    buffer[p11] = p11_;
                }
                else {
                    // 		    wenc16 (*px,  *p01, i00, i01);
                    // 		    wenc16 (*p10, *p11, i10, i11);
                    // 		    wenc16 (i00, i10, *px,  *p10);
                    // 		    wenc16 (i01, i11, *p01, *p11);
                    let (i00, i01) = encode_16bit(buffer[position_x], buffer[p01]);
                    let (i10, i11) = encode_16bit(buffer[p10], buffer[p11]);

                    let (px_, p10_) = encode_16bit(i00, i10);
                    let (p01_, p11_) = encode_16bit(i01, i11);

                    buffer[position_x] = px_; // TODO rustify
                    buffer[p10] = p10_;
                    buffer[p01] = p01_;
                    buffer[p11] = p11_;
                }

                position_x += offset2_x;
            }

            // 	    if (nx & p)
            // 		unsigned short *p10 = px + oy1;
            // 		if (w14) wenc14 (*px, *p10, i00, *p10);
            // 		else wenc16 (*px, *p10, i00, *p10);
            // 		*px= i00;

            // encode remaining odd pixel column
            if count_x & p != 0 {
                let p10 = position_x + offset1_y;
                let (i00, p10_) = {
                    if is_14_bit { encode_14bit(buffer[position_x], buffer[p10]) }
                    else { encode_16bit(buffer[position_x], buffer[p10]) }
                };

                buffer[position_x] = i00;
                buffer[p10] = p10_;
            }

            position_y += offset2_y;
        }

        // 	if (ny & p)
        // encode possibly remaining odd row
        if count_y & p != 0 {
            let mut position_x = position_y;
            let end_x = position_y + offset_x * (count_x - p2);

            // 	    unsigned short *px = py;
            // 	    unsigned short *ex = py + ox * (nx - p2);
            // 	    for (; px <= ex; px += ox2) {
            // 		   unsigned short *p01 = px + ox1;
            // 		   if (w14) wenc14 (*px, *p01, i00, *p01);
            // 		   else wenc16 (*px, *p01, i00, *p01);
            // 		   *px= i00;
            while position_x <= end_x {
                println!("odd x loop: position_x = {}, end = {}", position_x, end_x);

                let p01 = position_x + offset1_x;

                let (px_, p01_) = {
                    if is_14_bit { encode_14bit(buffer[position_x], buffer[p01]) }
                    else { encode_16bit(buffer[position_x], buffer[p01]) }
                };

                buffer[p01] = p01_;
                buffer[position_x] = px_;

                position_x += offset2_x;
            }
        }

        // 	p = p2;
        // 	p2 <<= 1;
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
    //     bool w14 = (mx < (1 << 14));
    //     int	n = (nx > ny)? ny: nx;
    //     int	p = 1;
    let is_14_bit = max < (1 << 14);
    let count = count_x.min(count_y);
    let mut p: usize = 1; // TODO i32?
    let mut p2: usize; // TODO i32?

    //     while (p <= n)
    // 	    p <<= 1;

    // search max level
    while p <= count {
        p <<= 1;
    }

    //     p >>= 1;
    //     p2 = p;
    //     p >>= 1;
    p >>= 1;
    p2 = p;
    p >>= 1;

    //     // Hierarchical loop on smaller dimension n
    //     while (p >= 1)
    while p >= 1 {

        // 	unsigned short *py = in;
        // 	unsigned short *ey = in + oy * (ny - p2);
        let mut position_y = 0;
        let end_y = 0 + offset_y * (count_y - p2);

        // 	int oy1 = oy * p;
        // 	int oy2 = oy * p2;
        // 	int ox1 = ox * p;
        // 	int ox2 = ox * p2;
        let (offset1_x, offset1_y) = (offset_x * p, offset_y * p);
        let (offset2_x, offset2_y) = (offset_x * p2, offset_y * p2);

        debug_assert_ne!(offset_x, 0, "offset is zero (but shouldnt be???)"); // ????
        debug_assert_ne!(offset_y, 0, "offset is zero (but shouldnt be???)"); // ????


        // 	unsigned short i00,i01,i10,i11;
        // 	for (; py <= ey; py += oy2)

        while position_y <= end_y {

            // 	    unsigned short *px = py;
            // 	    unsigned short *ex = py + ox * (nx - p2);
            let mut position_x = position_y;
            let end_x = position_x + offset_x * (count_x - p2);

            // 	    for (; px <= ex; px += ox2)
            while position_x <= end_x {

                // 		unsigned short *p01 = px  + ox1;
                // 		unsigned short *p10 = px  + oy1;
                // 		unsigned short *p11 = p10 + ox1;
                let p01 = position_x + offset1_x;
                let p10 = position_x + offset1_y;
                let p11 = p10 + offset1_x;

                assert!(position_x < buffer.len());
                assert!(p01 < buffer.len());
                assert!(p10 < buffer.len());
                assert!(p11 < buffer.len());

                // 		if (w14)
                // 		    wdec14 (*px,  *p10, i00, i10);
                // 		    wdec14 (*p01, *p11, i01, i11);
                // 		    wdec14 (i00, i01, *px,  *p01);
                // 		    wdec14 (i10, i11, *p10, *p11);
                if is_14_bit {
                    let (i00, i10) = decode_14bit(buffer[position_x], buffer[p10]);
                    let (i01, i11) = decode_14bit(buffer[p01], buffer[p11]);

                    let (px_, p01_) = decode_14bit(i00, i01);
                    let (p10_, p11_) = decode_14bit(i10, i11);

                    buffer[position_x] = px_; // TODO rustify
                    buffer[p10] = p10_;
                    buffer[p01] = p01_;
                    buffer[p11] = p11_;
                }

                // 		    wdec16 (*px,  *p10, i00, i10);
                // 		    wdec16 (*p01, *p11, i01, i11);
                // 		    wdec16 (i00, i01, *px,  *p01);
                // 		    wdec16 (i10, i11, *p10, *p11);
                else {
                    let (i00, i10) = decode_16bit(buffer[position_x], buffer[p10]);
                    let (i01, i11) = decode_16bit(buffer[p01], buffer[p11]);
                    let (px_, p01_) = decode_16bit(i00, i01);
                    let (p10_, p11_) = decode_16bit(i10, i11);

                    buffer[position_x] = px_; // TODO rustify
                    buffer[p10] = p10_;
                    buffer[p01] = p01_;
                    buffer[p11] = p11_;
                }

                position_x += offset2_x;
            }

            // decode last odd remaining x value
            if count_x & p != 0 {

            // 		unsigned short *p10 = px + oy1;
            // 		if (w14) wdec14 (*px, *p10, i00, *p10);
            // 		else wdec16 (*px, *p10, i00, *p10);
            // 		*px= i00;
                let p10 = position_x + offset1_y;
                let (px_, p10_) = {
                    if is_14_bit { decode_14bit(buffer[position_x], buffer[p10]) }
                    else { decode_16bit(buffer[position_x], buffer[p10]) }
                };

                buffer[position_x] = px_;
                buffer[p10] = p10_;
            }

            position_y += offset2_y;
        }

        // decode remaining odd row
        if count_y & p != 0 {
            let mut position_x = position_y;
            let end_x = position_x + offset_x * (count_x - p2);

            // 	    unsigned short *px = py;
            // 	    unsigned short *ex = py + ox * (nx - p2);
            // 	    for (; px <= ex; px += ox2) {
            // 		    unsigned short *p01 = px + ox1;
            // 		    if (w14) wdec14 (*px, *p01, i00, *p01);
            // 		    else wdec16 (*px, *p01, i00, *p01);
            // 		    *px= i00;

            while position_x <= end_x {
                let p01 = position_x + offset1_x;

                let (px_, p01_) = {
                    if is_14_bit { decode_14bit(buffer[position_x], buffer[p01]) }
                    else { decode_16bit(buffer[position_x], buffer[p01]) }
                };

                buffer[position_x] = px_;
                buffer[p01] = p01_;

                position_x += offset2_x;
            }

        }

        // 	p2 = p;
        // 	p >>= 1;
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