use std::time::Instant;
use image::ImageReader;
use std::io::Cursor;

use shader_test::composite::{params::CompositeParams, CompositeProcessor};

fn main() {
    let frame = include_bytes!("./meow.jpg");

    let img = ImageReader::new(Cursor::new(frame))
        .with_guessed_format()
        .expect("failed to guess format")
        .decode()
        .expect("failed to decode image");

    let width = img.width();
    let height = img.height();
    let rgba = img.to_rgba8();

    let mut processor = CompositeProcessor::new(CompositeParams::default(), width.try_into().unwrap(), height.try_into().unwrap());

    let start = Instant::now();
    for _ in 0..4000 {
        let (_rgba, _out_w, _out_h) = processor.process(rgba.as_raw(), width.try_into().unwrap(), height.try_into().unwrap());
    }
    let elapsed = start.elapsed();
    println!("4000 frames in {:?} ({:.2}ms/frame)", elapsed, elapsed.as_secs_f64() * 1000.0 / 4000.0);
}
