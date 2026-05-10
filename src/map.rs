use image::RgbImage;
use include_dir::{include_dir, Dir};

use crate::model::Map;
use crate::texture::FloorTexture;

enum ColorMap {
    Wall1Colour = 0x000000, // Black
    Wall2Colour = 0x0026FF, // BLUE
    Wall3Color = 0x00FF21,  // GREEN
    FloorSmooth = 0xFFFFFF, // WHITE
    FloorMilkVeins = 0xFD8EFF, // PINK
}

static TEXTURES_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/textures");

pub fn load_embedded_map() -> Map {
    let map_file = TEXTURES_DIR
        .get_file("map.png")
        .expect("Failed to find embedded map image: map.png");
    let img: RgbImage = image::load_from_memory(map_file.contents())
        .expect("Failed to decode embedded map image")
        .to_rgb8();

    map_from_image(&img)
}

fn map_from_image(img: &RgbImage) -> Map {

    let mut tiles: Vec<Vec<u8>> = Vec::new();
    let mut floor_tiles: Vec<Vec<FloorTexture>> = Vec::new();

    for y in 0..img.height() {
        let mut row: Vec<u8> = Vec::new();
        let mut floor_row: Vec<FloorTexture> = Vec::new();
        for x in 0..img.width() {
            let pixel = img.get_pixel(x, y);
            let color_value = (pixel[0] as u32) << 16 | (pixel[1] as u32) << 8 | (pixel[2] as u32);

            let (cell_value, floor_texture) = match color_value {
                x if x == ColorMap::Wall1Colour as u32 => (1, FloorTexture::Smooth),
                x if x == ColorMap::Wall2Colour as u32 => (2, FloorTexture::Smooth),
                x if x == ColorMap::Wall3Color as u32 => (3, FloorTexture::Smooth),
                x if x == ColorMap::FloorSmooth as u32 => (0, FloorTexture::Smooth),
                x if x == ColorMap::FloorMilkVeins as u32 => (0, FloorTexture::MilkVeins),
                _ => (0, FloorTexture::Smooth),
            };
            row.push(cell_value);
            floor_row.push(floor_texture);
        }
        tiles.push(row);
        floor_tiles.push(floor_row);
    }

    Map::with_floor_tiles(tiles, floor_tiles)
}
