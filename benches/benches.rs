#[macro_use]
extern crate bencher;

use bencher::Bencher;
use rs_exr::prelude as exr;

fn single_image(bench: &mut Bencher) {
    bench.iter(|| {
        let path = ::std::path::Path::new(
            "D:/Pictures/openexr/crowskull/crow_zips.exr"
        );

        let image = exr::read(path).unwrap();
        bencher::black_box(image);
        // println!(image.parts.len());
    })
}

benchmark_group!(benches, single_image);
benchmark_main!(benches);