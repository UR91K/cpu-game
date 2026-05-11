use image::{Rgba, RgbaImage};

use crate::renderer::mesh::AtlasRect;

pub fn build_texture_atlas(textures: &[RgbaImage]) -> (RgbaImage, Vec<AtlasRect>) {
    let padding = 1u32;
    let width = textures
        .iter()
        .map(|texture| texture.width() + padding)
        .sum::<u32>()
        + padding;
    let height = textures
        .iter()
        .map(image::GenericImageView::height)
        .max()
        .unwrap_or(1)
        + padding * 2;

    let mut atlas = RgbaImage::from_pixel(width.max(1), height.max(1), Rgba([0, 0, 0, 0]));
    let mut rects = Vec::with_capacity(textures.len());
    let mut cursor_x = padding;

    for texture in textures {
        for y in 0..texture.height() {
            for x in 0..texture.width() {
                let pixel = texture.get_pixel(x, y);
                atlas.put_pixel(cursor_x + x, padding + y, *pixel);
            }
        }

        rects.push(AtlasRect {
            u0: cursor_x as f32 / width as f32,
            v0: padding as f32 / height as f32,
            u1: (cursor_x + texture.width()) as f32 / width as f32,
            v1: (padding + texture.height()) as f32 / height as f32,
            pixel_width: texture.width(),
            pixel_height: texture.height(),
        });
        cursor_x += texture.width() + padding;
    }

    (atlas, rects)
}
