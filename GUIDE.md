# Guide

This document talks about the capabilities of OpenEXR and outlines the design of this library. 
In addition to reading this guide, you should also have a look at the examples.

## Wording
Some names in this library differ from the classic OpenEXR conventions.
For example, an OpenEXR "multipart" is called a file with multiple "layers" in this library.
The old OpenEXR "layers" are called "grouped channels" instead.

- `Image` Contains everything that an `.exr` file can contain. Includes metadata and multiple layers.
- `Layer` A grid of pixels that can be placed anywhere on the two-dimensional canvas
- `Channel` All samples of a single color component, such as red or blue. Also contains metadata.
- `Pixel` The color at an exact location in the image. Contains one sample for each channel.
- `Sample` The value (either f16, f32 or u32) of one channel at an exact location in the image.
            Usually a simple number, such as the red value of the bottom left pixel.
- `Grouped Channels` Multiple channels may be grouped my prepending the same prefix to the name.
                    This behaviour is opt-in; it has to be enabled explicitly.
                    By default, channels are stored in a plain list, and channel names are unmodified.

## OpenEXR
This image format supports some features that you won't find in other image formats.
As a consequence, an exr file cannot necessarily be converted to other formats, 
even when the loss of precision is acceptable. Furthermore, 
an arbitrary exr image may include possibly unwanted data. 
Supporting deep data, for example, might be unnecessary for some applications.

To read an image, `exrs` must know which parts of an image you want to end up with, 
and which parts of the file should be skipped. That's why you need
a little more code to read an exr file, compared to simpler file formats.

### Possibly Undesired Features
- Arbitrary Channels: 
  `CMYK`, `YCbCr`, `LAB`, `XYZ` channels might not be interesting for you, 
  maybe you only want to accept `RGBA` images
- Deep Data: Multiple colors per pixel might not be interesting for you 
- Resolution Levels: Mip Maps or Rip Maps might be unnecessary and can be skipped,
  loading only the full resolution image instead
<!-- - TODO: Meta Data: Skip reading meta data -->

# Simple Reading and Writing
There are a few very simple functions for the most common use cases.
For decoding an image file, use one of these functions 
from the `exr::image::read` module (data structure complexity increasing):

1. `read_first_rgba_layer_from_file(path, your_constructor, your_pixel_setter)`
1. `read_all_rgba_layers_from_file(path, your_constructor, your_pixel_setter)`
1. `read_first_flat_layer_from_file(path)`
1. `read_all_flat_layers_from_file(path)`
1. `read_all_data_from_file(path)`

If you don't have a file path, or want to load any other channels than `rgba`, 
then these simple functions will not suffice. The more complex approaches are
described later in this document.

For encoding an image file, use one of these functions in the `exr::image::write` module:

1. `write_rgba_file(path, width, height, |x,y| my_image.get_rgb_at(x,y))`
1. `write_rgb_file(path, width, height, |x,y| my_image.get_rgba_at(x,y))`

These functions are only syntactic sugar. If you want to customize the data type,
the compression method, or write multiple layers, these simple functions will not suffice.
Again, the more complex approaches are described in the following paragraph.

# Reading an Image

Reading an image involves three steps:
1. Specify how to load an image by constructing an image reader.
   Start with `read()`. Chain method calls on the result of this function to customize the reader.
1. Call `from_file(path)`, `from_buffered(bytes)`, or `from_unbuffered(bytes)` 
   on the reader to actually load an image
1. Process the resulting image data structure or the error in your application

Full example, loading all channels from the file:
```rust
fn main() {
    use exr::prelude::*;

    // the type of the this image depends on the chosen options
    let image = read()
        .no_deep_data() // (currently required)
        .largest_resolution_level() // or `all_resolution_levels()`
        .all_channels() // or `specific_channels() ...`
        .all_layers() // or `first_valid_layer()`
        .all_attributes() // (currently required)
        .on_progress(|progress| println!("progress: {:.1}", progress * 100.0)) // optional
        .non_parallel() // optional. discouraged. just leave this line out
        .from_file("image.exr").unwrap(); // or `from_buffered(my_byte_slice)`
}
```

Loading only specified channels:
```rust
fn main() {
    use exr::prelude::*;

    // the type of the this image depends on the chosen options
    let image = read()
        .no_deep_data() // (currently required)
        .largest_resolution_level() // or `all_resolution_levels()`

        // alternative to `all_channels()`
        .specific_channels().required("X").required("Y").required("Z").optional("A", 1.0).collect_pixels(
            // create our image with the resolution of the file
            |resolution, (x_channel, y_channel, z_channel, a_channel)|{
                if a_channel.is_some() { MyImage::with_alpha(resolution) }
                else { MyImage::without_alpha(resolution) }
            },
        
            // define how to insert a single pixel
            // define that x should be converted to f32, and y and z should be converted to f16
            // and alpha should not be converted (may be either f16, f32, or u32)
            |my_image, pos, (x,y,z,a): (f32,f16,f16,Sample)|{
                my_image.set_color(pos.x(), pos.y(), (x,y,z,a.to_f32()));
            }
        )   
        
        .all_layers() // or `first_valid_layer()`
        .all_attributes() // (currently required)
        .on_progress(|progress| println!("progress: {:.1}", progress * 100.0)) // optional
        .non_parallel() // optional. discouraged. just leave this line out
        .from_file("image.exr").unwrap(); // or `from_buffered(my_byte_slice)`
}
```


# Writing an Image

Writing an image involves three steps:
1. Construct the image data structure, starting with an `exrs::image::Image`
1. Call `image_data.write()` to obtain an image writer
1. Customize the writer, for example in order to listen for the progress
1. Write the image by calling `to_file(path)`, `to_buffered(bytes)`, or `to_unbuffered(bytes)` on the reader

Full example, writing a flexible list of channels: 
````rust
fn main(){
    // construct an image to write
    let image = Image::from_single_layer(
        Layer::new( // the only layer in this image
            (1920, 1080), // resolution
            LayerAttributes::named("main-rgb-layer"), // the layer has a name and other properties
            Encoding::FAST_LOSSLESS, // compress slightly 
            AnyChannels::sort(smallvec![ // the channels contain the actual pixel data
                AnyChannel::new("R", FlatSamples::F32(vec![0.6; 1920*1080 ])), // this channel contains all red values
                AnyChannel::new("G", FlatSamples::F32(vec![0.7; 1920*1080 ])), // this channel contains all green values
                AnyChannel::new("B", FlatSamples::F32(vec![0.9; 1920*1080 ])), // this channel contains all blue values
            ]),
        )
    );

    image.write()
        .on_progress(|progress| println!("progress: {:.1}", progress*100.0)) // optional
        .non_parallel() // optional. discouraged. just leave this line out
        .to_file("image.exr").unwrap();
}
````

Writing a fixed set of channels: 
````rust
fn main(){
    let my_image = unimplemented!();
    
    // construct an image to write
    let image = Image::from_single_layer(
        Layer::new( // the only layer in this image
            (my_image.width(), my_image.height()), // resolution
            LayerAttributes::named("main-luma-layer"), // the layer has a name and other properties
            Encoding::FAST_LOSSLESS, // compress slightly 
            
            SpecificChannels::build().with_channel("Y").with_channel("A").with_pixel_fn(|position| {
                let (luma, alpha) = my_image.get_pixel(position.x(), position.y());
                (luma as f32, f16::from_f32(alpha)) // store f32 luma and f16 alpha samples in the file
            }),
        )
    );

    image.write()
        .on_progress(|progress| println!("progress: {:.1}", progress*100.0)) // optional
        .non_parallel() // optional. discouraged. just leave this line out
        .to_file("image.exr").unwrap();
}
````

### Pixel Closures
When working with specific channels, the data is not stored directly.
Instead, you provide a closure that stores or loads pixels in your existing image data structure.

If you really do not want to provide your own storage, you can use the predefined structures from
`exr::image::pixel_vec`, such as `PixelVec<(f32,f32,f16)>` or `create_pixel_vec`.
Use this only if you don't already have a pixel storage.

# The `Image` Data Structure

For great flexibility, this crate does not offer a plain data structure to represent an exr image.
Instead, the `Image` data type has a generic parameter, allowing for different image contents.

````rust
fn main(){
    // this image contains only a single layer
    let single_layer_image: Image<Layer<_>> = Image::from_single_layer(my_layer);

    // this image contains an arbitrary number of layers
    let multi_layer_image: Image<Layers<_>> = Image::new(attributes, smallvec![ layer1, layer2 ]);

    // this image can only contain rgb or rgba channels
    let single_layer_rgb_image : Image<Layer<RgbaChannels<_>>> = Image::from_single_layer(Layer::new(
        dimensions, attributes, encoding,
        RgbaChannels::new(sample_types, rgba_pixels)
    ));
    
    // this image can contain arbitrary channels, such as LAB or YCbCr
    let single_layer_image : Image<Layer<AnyChannels<_>>> = Image::from_single_layer(Layer::new(
        dimensions, attributes, encoding,
        AnyChannels::sort(smallvec![ channel_x, channel_y, channel_z ])
    ));
    
}
````

The following pseudo code illustrates the image data structure.
The image should always be constructed using the constructor functions such as `Image::new(...)`,
because these functions watch out for invalid image contents.

````
Image {
    attributes: ImageAttributes,
    
    // the layer data can be either a single layer a list of layers
    layer_data: Layer | SmallVec<Layer>,
}

Layer {
    
    // the channel data can either be a fixed set of known channels, or a list of arbitrary channels
    channel_data: SpecificChannels | AnyChannels,
    
    attributes: LayerAttributes,
    size: Vec2<usize>,
    encoding: Encoding,
}

SpecificChannels {
    channels: [any tuple containing `ChannelDescription` or `Option<ChannelDescription>`],
    
    // the storage is usually a closure or a custom type which implements the `GetPixel` trait
    storage: impl GetPixel | impl Fn(Vec2<usize>) -> Pixel,    
        where Pixel = any tuple containing f16 or f32 or u32 values
}

AnyChannels {
    list: SmallVec<AnyChannel>
}

AnyChannel {
    name: Text,
    sample_data: FlatSamples | Levels,
    quantize_linearly: bool,
    sampling: Vec2<usize>,
}

Levels = Singular(FlatSamples) | Mip(FlatSamples) | Rip(FlatSamples)
FlatSamples = F16(Vec<f16>) | F32(Vec<f32>) | U32(Vec<u32>)
````

As a consequence, one of the simpler image types is `Image<Layer<AnyChannels<FlatSamples>>>`. If you
enable loading multiple resolution levels, you will instead get the type `Image<Layer<AnyChannels<Levels<FlatSamples>>>>`.

While you can put anything inside an image, 
it can only be written if the content of the image implements certain traits.
This allows you to potentially write your own channel storage system.
