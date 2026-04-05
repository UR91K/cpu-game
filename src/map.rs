use image::{ImageBuffer, RgbImage};

enum COLOR_MAP {
    WALL1_COLOR = 0x000000, // Black
    WALL2_COLOR = 0x0026FF, // BLUE
    WALL3_COLOR = 0x00FF21, // GREEN
    EMPTY_COLOR = 0xFFFFFF, // White
}

pub fn load_map(file_path: &str) -> Vec<Vec<u8>> {
    let img: RgbImage = image::open(file_path)
        .expect("Failed to open map image")
        .to_rgb8();

    let mut map: Vec<Vec<u8>> = Vec::new();

    for y in 0..img.height() {
        let mut row: Vec<u8> = Vec::new();
        for x in 0..img.width() {
            let pixel = img.get_pixel(x, y);
            let color_value = (pixel[0] as u32) << 16 | (pixel[1] as u32) << 8 | (pixel[2] as u32);

            let cell_value = match color_value {
                x if x == COLOR_MAP::WALL1_COLOR as u32 => 1,
                x if x == COLOR_MAP::WALL2_COLOR as u32 => 2,
                x if x == COLOR_MAP::WALL3_COLOR as u32 => 3,
                _ => 0, // Default to empty space
            };
            row.push(cell_value);
        }
        map.push(row);
    }

    map
}