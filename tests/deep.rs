//! Tests for reading and writing deep images.

extern crate exr;

use std::{convert::TryInto, ffi::OsStr, io::Cursor, path::PathBuf};

use exr::{
    image::deep::AnyDeepChannels,
    meta::attribute::DeepImageState,
    prelude::{pixel_vec::PixelVec, *},
};

const LOW_RES: &str = "tests/images/valid/openexr/v2/LowResLeftView";

fn deep_files_in_repository() -> Vec<PathBuf> {
    let files: Vec<PathBuf> = walkdir::WalkDir::new("tests/images/valid")
        .into_iter()
        .map(std::result::Result::unwrap)
        .filter(|entry| entry.path().extension() == Some(OsStr::new("exr")))
        .map(walkdir::DirEntry::into_path)
        .filter(|path| {
            MetaData::read_from_file(path, false)
                .is_ok_and(|meta| meta.headers.iter().any(|header| header.deep))
        })
        .collect();

    assert!(files.len() >= 12, "not enough deep images found: {:?}", files);
    files
}

fn read_deep_bytes(bytes: &[u8]) -> Result<DeepLayersImage> {
    read().deep_data().all_layers().pedantic().from_buffered(Cursor::new(bytes))
}

fn write_to_bytes(image: &DeepLayersImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image.write().to_buffered(Cursor::new(&mut bytes)).expect("writing deep image failed");
    bytes
}

/// Compare the deep data and the relevant attributes of two images.
fn assert_deep_images_equal(expected: &DeepLayersImage, actual: &DeepLayersImage) {
    assert_eq!(expected.attributes, actual.attributes);
    assert_eq!(expected.layer_data.len(), actual.layer_data.len());

    for (expected, actual) in expected.layer_data.iter().zip(&actual.layer_data) {
        assert_eq!(expected.size, actual.size);
        assert_eq!(expected.attributes, actual.attributes);
        assert_eq!(expected.channel_data, actual.channel_data);
    }
}

#[test]
fn read_all_deep_images_in_repository() {
    for path in deep_files_in_repository() {
        let parallel = read().deep_data().all_layers().from_file(&path).unwrap();
        let sequential =
            read().deep_data().all_layers().non_parallel().pedantic().from_file(&path).unwrap();

        assert_deep_images_equal(&parallel, &sequential);

        for layer in &parallel.layer_data {
            let deep = &layer.channel_data;
            assert_eq!(deep.resolution(), layer.size);
            assert!(deep.total_sample_count() > 0, "{:?} contains no samples", path);

            let names: Vec<String> =
                deep.channels().iter().map(|channel| channel.name.to_string()).collect();
            assert_eq!(names, vec!["A", "B", "G", "R", "Z"], "{path:?}");
        }

        let first = exr::image::read::deep::read_first_deep_layer_from_file(&path).unwrap();
        assert_eq!(first.layer_data.channel_data, parallel.layer_data[0].channel_data);
    }
}

#[test]
fn read_known_sample_values() {
    let image =
        read().deep_data().first_valid_layer().from_file(format!("{LOW_RES}/Balls.exr")).unwrap();

    let layer = &image.layer_data;
    assert_eq!(layer.attributes.layer_name, Some(Text::from("rgba.left")));
    assert_eq!(layer.attributes.layer_position, Vec2(131, 170));
    assert_eq!(layer.size, Vec2(764, 406));
    assert_eq!(layer.encoding.compression, Compression::ZIP1);

    // reference values were obtained using the `OpenEXR` library
    let deep = &layer.channel_data;
    let statistics = deep.statistics();
    assert_eq!(statistics.total_sample_count, 162_877);
    assert_eq!(statistics.max_samples_per_pixel, 2);
    assert_eq!(statistics.non_empty_pixel_count, 117_256);

    let samples = deep.sample_range(Vec2(430, 0));
    assert_eq!(samples.len(), 2);

    let values = |name: &str| -> Vec<f32> {
        let channel = deep.channel_by_name(name).unwrap();
        samples
            .clone()
            .map(|index| channel.sample_data.value_by_flat_index(index).to_f32())
            .collect()
    };

    assert_eq!(values("A"), vec![0.109375, 0.017547607421875]);
    assert_eq!(values("R"), vec![0.0261077880859375, 0.003978729248046875]);
    assert_eq!(values("Z"), vec![698.0758666992188, 699.7129516601562]);
    assert_eq!(deep.sample_count(Vec2(0, 0)), 0);
}

#[test]
fn flat_and_deep_readers_reject_each_other() {
    let deep_path = format!("{LOW_RES}/Balls.exr");

    // flat readers report deep data as unsupported instead of invalid
    assert!(matches!(read_all_data_from_file(&deep_path), Err(Error::NotSupported(_))));
    assert!(matches!(read_all_flat_layers_from_file(&deep_path), Err(Error::NotSupported(_))));
    assert!(matches!(
        read_first_rgba_layer_from_file(
            &deep_path,
            |_, _| (),
            |_: &mut (), _: Vec2<usize>, _: (f32, f32, f32, f32)| {}
        ),
        Err(Error::NotSupported(_))
    ));

    // deep readers report flat data as unsupported
    let flat_path = format!("{LOW_RES}/composited.exr");
    assert!(matches!(
        read().deep_data().first_valid_layer().from_file(&flat_path),
        Err(Error::NotSupported(_))
    ));
    assert!(matches!(
        read().deep_data().all_layers().from_file(&flat_path),
        Err(Error::NotSupported(_))
    ));
}

#[test]
fn round_trip_with_all_compression_methods_and_blocks() {
    let original =
        read().deep_data().all_layers().from_file(format!("{LOW_RES}/Trunks.exr")).unwrap();

    for compression in [Compression::Uncompressed, Compression::RLE, Compression::ZIP1] {
        for blocks in [Blocks::ScanLines, Blocks::Tiles(Vec2(64, 64)), Blocks::Tiles(Vec2(37, 23))]
        {
            let mut image = original.clone();
            image.layer_data[0].encoding = Encoding {
                compression,
                blocks,
                line_order: LineOrder::Increasing,
            };

            let bytes = write_to_bytes(&image);
            let decoded = read_deep_bytes(&bytes).unwrap();
            assert_deep_images_equal(&image, &decoded);
            assert_eq!(decoded.layer_data[0].encoding, image.layer_data[0].encoding);

            let sequential = read()
                .deep_data()
                .all_layers()
                .non_parallel()
                .from_buffered(Cursor::new(&bytes))
                .unwrap();
            assert_deep_images_equal(&image, &sequential);
        }
    }
}

#[test]
fn round_trip_multi_part_file() {
    let path = "tests/images/valid/openexr/v2/Stereo/Balls.exr";
    let original = read().deep_data().all_layers().from_file(path).unwrap();
    assert_eq!(original.layer_data.len(), 2);

    let decoded = read_deep_bytes(&write_to_bytes(&original)).unwrap();
    assert_deep_images_equal(&original, &decoded);
}

/// Create a deep layer with pseudo-random contents, including empty pixels
/// and all sample types.
fn synthetic_layer(
    name: &str,
    size: Vec2<usize>,
    seed: u32,
    encoding: Encoding,
) -> Layer<AnyDeepChannels> {
    let mut state = seed;
    let mut random = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };

    // leave the first row empty, so that an entire block contains no samples
    let counts: Vec<usize> = (0..size.area())
        .map(|index| {
            if index < size.width() {
                0
            } else {
                (random() % 6) as usize
            }
        })
        .collect();

    let total: usize = counts.iter().sum();
    let depth = (0..total).map(|_| (random() % 10_000) as f32 * 0.25).collect();
    let alpha = (0..total).map(|_| f16::from_f32((random() % 1000) as f32 / 1000.0)).collect();
    let ids = (0..total).map(|_| random()).collect();

    let channels = AnyDeepChannels::new(
        size,
        counts,
        vec![
            AnyChannel::new("Z", FlatSamples::F32(depth)),
            AnyChannel::new("A", FlatSamples::F16(alpha)),
            AnyChannel::new("object_id", FlatSamples::U32(ids)),
        ],
    )
    .unwrap();

    Layer::new(size, LayerAttributes::named(name), encoding, channels)
}

#[test]
fn round_trip_synthetic_layers() {
    let encodings = [
        Encoding::UNCOMPRESSED,
        Encoding {
            compression: Compression::RLE,
            blocks: Blocks::Tiles(Vec2(5, 3)),
            line_order: LineOrder::Increasing,
        },
        Encoding {
            compression: Compression::ZIP1,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Decreasing,
        },
        Encoding {
            compression: Compression::ZIP1,
            blocks: Blocks::Tiles(Vec2(8, 8)),
            line_order: LineOrder::Unspecified,
        },
    ];

    let layers: Layers<AnyDeepChannels> = encodings
        .iter()
        .enumerate()
        .map(|(index, &encoding)| {
            let layer = synthetic_layer(
                &format!("layer {index}"),
                Vec2(17 + index, 11),
                index as u32 + 1,
                encoding,
            );
            Layer {
                attributes: layer.attributes.with_position(Vec2(index as i32 * 3 - 4, 2)),
                ..layer
            }
        })
        .collect();

    let image = Image::from_layers(ImageAttributes::with_size((40, 20)), layers);

    for parallel in [true, false] {
        let mut bytes = Vec::new();
        let writer = image.write();
        let writer = if parallel {
            writer
        } else {
            writer.non_parallel()
        };
        writer.to_buffered(Cursor::new(&mut bytes)).unwrap();

        let decoded = read_deep_bytes(&bytes).unwrap();
        assert_deep_images_equal(&image, &decoded);

        let meta = MetaData::read_from_buffered(bytes.as_slice(), true).unwrap();
        assert!(meta.requirements.has_deep_data);
        assert!(!meta.requirements.is_single_layer_and_tiled);

        for (header, layer) in meta.headers.iter().zip(&image.layer_data) {
            assert!(header.deep);
            assert_eq!(header.deep_data_version, Some(1));
            assert_eq!(
                header.max_samples_per_pixel,
                Some(layer.channel_data.max_samples_per_pixel())
            );
        }
    }
}

#[test]
fn round_trip_single_tiled_layer() {
    // a single tiled deep layer must not set the single-tile flag of the file
    let tiles = Encoding {
        compression: Compression::RLE,
        blocks: Blocks::Tiles(Vec2(4, 4)),
        line_order: LineOrder::Increasing,
    };
    let layer = synthetic_layer("deep", Vec2(9, 7), 7, tiles);
    let image = Image::from_layer(layer);

    let mut bytes = Vec::new();
    image.write().to_buffered(Cursor::new(&mut bytes)).unwrap();

    let meta = MetaData::read_from_buffered(bytes.as_slice(), true).unwrap();
    assert!(meta.requirements.has_deep_data);
    assert!(!meta.requirements.is_single_layer_and_tiled);

    let decoded: DeepImage =
        read().deep_data().first_valid_layer().from_buffered(Cursor::new(&bytes)).unwrap();
    assert_eq!(decoded.layer_data.channel_data, image.layer_data.channel_data);
}

#[test]
fn write_image_with_flat_and_deep_layers() {
    let size = Vec2(6, 4);
    let flat = Layer::new(
        size,
        LayerAttributes::named("flat"),
        Encoding::FAST_LOSSLESS,
        AnyChannels::sort(smallvec::smallvec![AnyChannel::new(
            "Y",
            FlatSamples::F32((0..size.area()).map(|index| index as f32).collect())
        )]),
    );

    let deep = synthetic_layer("deep", size, 3, Encoding::UNCOMPRESSED);
    let image = Image::empty(ImageAttributes::with_size(size))
        .with_layer(flat.clone())
        .with_layer(deep.clone());

    for parallel in [true, false] {
        let mut bytes = Vec::new();
        let writer = image.write();
        let writer = if parallel {
            writer
        } else {
            writer.non_parallel()
        };
        writer.to_buffered(Cursor::new(&mut bytes)).unwrap();

        let decoded_deep: DeepImage = read()
            .deep_data()
            .first_valid_layer()
            .pedantic()
            .from_buffered(Cursor::new(&bytes))
            .unwrap();
        assert_eq!(decoded_deep.layer_data.channel_data, deep.channel_data);

        let decoded_flat = read()
            .no_deep_data()
            .largest_resolution_level()
            .all_channels()
            .first_valid_layer()
            .all_attributes()
            .pedantic()
            .from_buffered(Cursor::new(&bytes))
            .unwrap();

        assert_eq!(decoded_flat.layer_data.channel_data, flat.channel_data);

        // the image contains a flat layer, so not all layers can be read as
        // deep data
        assert!(matches!(read_deep_bytes(&bytes), Err(Error::NotSupported(_))));
    }
}

#[test]
fn read_images_with_deep_and_flat_layers_in_any_order() {
    let size = Vec2(6, 4);
    let flat = Layer::new(
        size,
        LayerAttributes::named("flat"),
        Encoding::FAST_LOSSLESS,
        AnyChannels::sort(smallvec::smallvec![AnyChannel::new(
            "Y",
            FlatSamples::F32((0..size.area()).map(|index| index as f32).collect())
        )]),
    );

    let first_deep = synthetic_layer("first deep", size, 3, Encoding::UNCOMPRESSED);
    let second_deep = synthetic_layer("second deep", size, 4, Encoding::FAST_LOSSLESS);

    // the deep layer comes first, so flat readers must skip it
    let image = Image::empty(ImageAttributes::with_size(size))
        .with_layer(first_deep.clone())
        .with_layer(flat.clone())
        .with_layer(second_deep.clone());

    let mut bytes = Vec::new();
    image.write().to_buffered(Cursor::new(&mut bytes)).unwrap();

    let decoded_flat = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .first_valid_layer()
        .all_attributes()
        .pedantic()
        .from_buffered(Cursor::new(&bytes))
        .unwrap();

    assert_eq!(decoded_flat.layer_data.attributes.layer_name, flat.attributes.layer_name);
    assert_eq!(decoded_flat.layer_data.channel_data, flat.channel_data);

    let decoded_first: DeepImage = read()
        .deep_data()
        .first_valid_layer()
        .pedantic()
        .from_buffered(Cursor::new(&bytes))
        .unwrap();

    assert_eq!(decoded_first.layer_data.channel_data, first_deep.channel_data);

    // all deep layers can be read, skipping the flat layer
    let decoded_deep: DeepLayersImage = read()
        .deep_data()
        .all_valid_layers()
        .pedantic()
        .from_buffered(Cursor::new(&bytes))
        .unwrap();

    assert_eq!(decoded_deep.layer_data.len(), 2);
    assert_eq!(decoded_deep.layer_data[0].channel_data, first_deep.channel_data);
    assert_eq!(decoded_deep.layer_data[1].channel_data, second_deep.channel_data);
    assert_eq!(decoded_deep.layer_data[1].attributes.layer_name, second_deep.attributes.layer_name);
    assert!(matches!(read_deep_bytes(&bytes), Err(Error::NotSupported(_))));
}

/// Selects the deep layer with a specific name.
struct ReadDeepLayerByName(&'static str);

impl exr::image::read::deep::ReadDeepLayers for ReadDeepLayerByName {
    type Layers = Layer<AnyDeepChannels>;

    fn select_layers(&self, headers: &[exr::meta::header::Header]) -> Result<SmallVec<[usize; 2]>> {
        headers
            .iter()
            .position(|header| header.own_attributes.layer_name == Some(Text::from(self.0)))
            .map(|index| SmallVec::from_slice(&[index]))
            .ok_or_else(|| Error::Invalid("no layer with that name".into()))
    }

    fn collect_layers(&self, layers: Vec<Layer<AnyDeepChannels>>) -> Self::Layers {
        layers.into_iter().next().unwrap()
    }
}

#[test]
fn read_deep_layers_with_custom_selection() {
    let path = "tests/images/valid/openexr/v2/Stereo/Balls.exr";
    let all = read().deep_data().all_layers().from_file(path).unwrap();

    let second_name = all.layer_data[1].attributes.layer_name.clone().unwrap();
    let second_name: &'static str = Box::leak(second_name.to_string().into_boxed_str());

    let second = exr::image::read::deep::ReadDeepImage::new(ReadDeepLayerByName(second_name))
        .from_file(path)
        .unwrap();

    assert_eq!(second.layer_data.channel_data, all.layer_data[1].channel_data);
}

#[test]
fn version_flags_of_deep_files() {
    // clear the deep data flag, which is bit 11 of the version field
    let without_deep_flag = |path: &str| {
        let mut bytes = std::fs::read(path).unwrap();
        bytes[5] &= !0x08;
        bytes
    };

    // like `OpenEXR` 3.3 and newer, the type attribute of multi-layer files is
    // trusted, but not when reading pedantically, as older versions reject it
    let multi_layer = without_deep_flag("tests/images/valid/openexr/v2/Stereo/Balls.exr");
    let relaxed = read().deep_data().all_layers().from_buffered(Cursor::new(&multi_layer));
    assert_eq!(relaxed.unwrap().layer_data.len(), 2);
    assert!(read_deep_bytes(&multi_layer).is_err());

    // single-layer files must declare deep data
    let single_layer = without_deep_flag(&format!("{LOW_RES}/Balls.exr"));
    let relaxed = read().deep_data().all_layers().from_buffered(Cursor::new(&single_layer));
    assert!(relaxed.is_err());
}

#[test]
fn duplicate_chunk_offsets_are_read_once() {
    let encoding = Encoding {
        compression: Compression::ZIP1,
        blocks: Blocks::ScanLines,
        line_order: LineOrder::Increasing,
    };

    let layer = synthetic_layer("deep", Vec2(7, 16), 9, encoding);
    let image = Image::from_layer(layer);

    let mut bytes = Vec::new();
    image.write().to_buffered(Cursor::new(&mut bytes)).unwrap();

    // the offset table of the single layer is directly followed by the first
    // chunk
    let chunk_count = 16;
    let read_u64 = |bytes: &[u8], position: usize| {
        u64::from_le_bytes(bytes[position..position + 8].try_into().unwrap()) as usize
    };

    let table_start = (8..bytes.len() - 8)
        .find(|&position| read_u64(&bytes, position) == position + chunk_count * 8)
        .expect("offset table not found");

    // point all offsets to the first chunk
    let first_chunk = read_u64(&bytes, table_start);
    for index in 0..chunk_count {
        let position = table_start + index * 8;
        bytes[position..position + 8].copy_from_slice(&(first_chunk as u64).to_le_bytes());
    }

    // the first chunk only contains the first row, which contains no samples
    let relaxed: DeepImage =
        read().deep_data().first_valid_layer().from_buffered(Cursor::new(&bytes)).unwrap();
    assert_eq!(relaxed.layer_data.channel_data.total_sample_count(), 0);

    let pedantic =
        read().deep_data().first_valid_layer().pedantic().from_buffered(Cursor::new(&bytes));
    assert!(pedantic.is_err());
}

#[test]
fn overlapping_chunks_are_rejected() {
    let encoding = Encoding {
        compression: Compression::Uncompressed,
        blocks: Blocks::ScanLines,
        line_order: LineOrder::Increasing,
    };

    let layer = synthetic_layer("deep", Vec2(7, 16), 9, encoding);
    let image = Image::from_layer(layer);

    let mut bytes = Vec::new();
    image.write().to_buffered(Cursor::new(&mut bytes)).unwrap();

    let chunk_count = 16;
    let read_u64 = |bytes: &[u8], position: usize| {
        u64::from_le_bytes(bytes[position..position + 8].try_into().unwrap()) as usize
    };

    let table_start = (8..bytes.len() - 8)
        .find(|&position| read_u64(&bytes, position) == position + chunk_count * 8)
        .expect("offset table not found");

    // point the second offset into the first chunk, such that both chunks
    // would contain the same bytes
    let first_chunk = read_u64(&bytes, table_start);
    let second_offset = table_start + 8;
    bytes[second_offset..second_offset + 8]
        .copy_from_slice(&(first_chunk as u64 + 8).to_le_bytes());

    for pedantic in [false, true] {
        let reader = read().deep_data().first_valid_layer();
        let result: Result<DeepImage> = if pedantic {
            reader.pedantic().from_buffered(Cursor::new(&bytes))
        } else {
            reader.from_buffered(Cursor::new(&bytes))
        };

        assert!(
            matches!(&result, Err(Error::Invalid(message)) if message.contains("overlapping")),
            "{:?}",
            result.map(|image| image.layer_data.channel_data.total_sample_count())
        );
    }
}

#[test]
fn read_chunks_after_skipping_tiny_chunks() {
    // the single chunk of a flat layer with one pixel has only 14 bytes,
    // and is skipped without seeking when reading the other layers
    let tiny = Layer::new(
        Vec2(1, 1),
        LayerAttributes::named("tiny"),
        Encoding::UNCOMPRESSED,
        AnyChannels::sort(smallvec::smallvec![AnyChannel::new(
            "Y",
            FlatSamples::F16(vec![f16::ONE])
        )]),
    );

    let first = synthetic_layer("first", Vec2(3, 4), 5, Encoding::UNCOMPRESSED);
    let second = synthetic_layer("second", Vec2(3, 4), 6, Encoding::UNCOMPRESSED);

    let image = Image::empty(ImageAttributes::with_size(Vec2(3, 4)))
        .with_layer(first.clone())
        .with_layer(tiny.clone())
        .with_layer(second.clone());

    let mut bytes = Vec::new();
    image.write().non_parallel().to_buffered(Cursor::new(&mut bytes)).unwrap();

    for pedantic in [false, true] {
        let reader = read().deep_data().all_valid_layers();
        let reader = if pedantic {
            reader.pedantic()
        } else {
            reader
        };

        let decoded = reader.from_buffered(Cursor::new(&bytes)).unwrap();
        assert_eq!(decoded.layer_data.len(), 2);
        assert_eq!(decoded.layer_data[0].channel_data, first.channel_data);
        assert_eq!(decoded.layer_data[1].channel_data, second.channel_data);
    }

    // reading flat layers of an image with deep layers
    let flat = Layer::new(
        Vec2(2, 3),
        LayerAttributes::named("flat"),
        Encoding::UNCOMPRESSED,
        AnyChannels::sort(smallvec::smallvec![AnyChannel::new(
            "R",
            FlatSamples::F16((0..6).map(|value| f16::from_f32(value as f32)).collect())
        )]),
    );

    let image = Image::empty(ImageAttributes::with_size(Vec2(3, 4)))
        .with_layer(tiny)
        .with_layer(flat)
        .with_layer(first);

    let mut bytes = Vec::new();
    image.write().non_parallel().to_buffered(Cursor::new(&mut bytes)).unwrap();

    let decoded = read()
        .no_deep_data()
        .largest_resolution_level()
        .specific_channels()
        .required("R")
        .collect_pixels(PixelVec::<(f16,)>::constructor, PixelVec::set_pixel)
        .first_valid_layer()
        .all_attributes()
        .from_buffered(Cursor::new(&bytes))
        .unwrap();

    let values: Vec<f32> =
        decoded.layer_data.channel_data.pixels.pixels.iter().map(|(red,)| red.to_f32()).collect();

    assert_eq!(values, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
}

#[test]
fn deep_image_state_attribute() {
    let mut layer = synthetic_layer("deep", Vec2(3, 3), 11, Encoding::UNCOMPRESSED);
    layer.attributes.deep_image_state = Some(DeepImageState::Tidy);

    let image = Image::from_layer(layer);
    let mut bytes = Vec::new();
    image.write().to_buffered(Cursor::new(&mut bytes)).unwrap();

    // the attribute is stored as a single byte with the type `deepImageState`,
    // as in `OpenEXR`
    let encoded = b"deepImageState\0deepImageState\0\x01\0\0\0\x03";
    assert!(bytes.windows(encoded.len()).any(|window| window == encoded));

    let decoded: DeepImage = read()
        .deep_data()
        .first_valid_layer()
        .pedantic()
        .from_buffered(Cursor::new(&bytes))
        .unwrap();
    assert_eq!(decoded.layer_data.attributes.deep_image_state, Some(DeepImageState::Tidy));
}

#[test]
fn reject_invalid_deep_layers_when_writing() {
    let layer = synthetic_layer("deep", Vec2(4, 4), 5, Encoding::UNCOMPRESSED);

    // the layer size must match the resolution of the deep channels
    let wrong_size = Layer {
        size: Vec2(5, 4),
        ..layer.clone()
    };
    let mut bytes = Vec::new();
    assert!(Image::from_layer(wrong_size).write().to_buffered(Cursor::new(&mut bytes)).is_err());

    // only some compression methods support deep data
    let piz = Layer {
        encoding: Encoding {
            compression: Compression::PIZ,
            ..Encoding::UNCOMPRESSED
        },
        ..layer
    };
    let mut bytes = Vec::new();
    assert!(Image::from_layer(piz).write().to_buffered(Cursor::new(&mut bytes)).is_err());
}
