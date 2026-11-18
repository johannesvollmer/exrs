//! Without the `htj2k` feature, writing HTJ2K and LJ2K fails as reading them
//! does: with `Error::NotSupported` naming the feature, never a panic.

#![cfg(not(feature = "htj2k"))]

use std::io::Cursor;

use exr::prelude::*;

#[test]
fn writing_htj2k_needs_the_feature() {
    for compression in [Compression::HTJ2K32, Compression::HTJ2K256, Compression::LJ2K(None)] {
        let channel = AnyChannel::new("Y", FlatSamples::F16(vec![f16::ONE; 16 * 16]));
        let encoding = Encoding {
            compression,
            ..Encoding::default()
        };
        let layer = Layer::new(
            (16, 16),
            LayerAttributes::default(),
            encoding,
            AnyChannels::sort(smallvec::smallvec![channel]),
        );

        let mut bytes = Vec::new();
        match Image::from_layer(layer).write().to_buffered(Cursor::new(&mut bytes)) {
            Err(Error::NotSupported(message)) => {
                assert!(message.contains("`htj2k` feature"), "{}", message);
            }
            other => panic!("{:?}: {:?}", compression, other),
        }
    }
}

#[test]
fn reading_htj2k_needs_the_feature() {
    let path = "tests/images/valid/custom/htj2k/y_half_htj2k32.exr";
    let result = read_all_flat_layers_from_file(path);
    match result {
        Err(Error::NotSupported(message)) => {
            assert!(message.contains("`htj2k` feature"), "{}", message);
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
}
