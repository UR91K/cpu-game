use std::time::Instant;
use image::ImageReader;
use std::io::Cursor;

use shader_test::composite::{params::CompositeParams, CompositeProcessor};

fn main() {
    let frame = include_bytes!("./meow.jpg");

    // Decode the JPEG to get dimensions and raw RGBA bytes
    let img = ImageReader::new(Cursor::new(frame))
        .with_guessed_format()
        .expect("failed to guess format")
        .decode()
        .expect("failed to decode image");

    let width = img.width();
    let height = img.height();

    let mut processor = CompositeProcessor::new(CompositeParams::default(), width, height);

    let start = Instant::now();
    let (rgba, out_w, out_h) = processor.process(&frame, width, height);
    let elapsed = start.elapsed();

    println!("processed 1 frame in {:?}", elapsed);
}