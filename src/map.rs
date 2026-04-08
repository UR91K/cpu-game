use image::RgbImage;

use crate::model::{AoField, AoParameters, Map};

enum ColorMap {
    Wall1Colour = 0x000000, // Black
    Wall2Colour = 0x0026FF, // BLUE
    Wall3Color = 0x00FF21,  // GREEN
}

pub fn build_ao(map: &Map, params: &AoParameters) -> AoField {
    let height = map.tiles.len();
    let width = map.tiles.first().map_or(0, |row| row.len());

    if width == 0 || height == 0 {
        return AoField {
            width,
            height,
            corners: Vec::new(),
        };
    }

    let mut corners = vec![[255u8; 4]; width * height];

    let min_light = params.minimum_light.clamp(0.0, 1.0);
    let strength = params.corner_strength.clamp(0.0, 1.0);
    let _wall_seam_strength = params.wall_seam_strength.clamp(0.0, 1.0);

    let wall_at = |x: isize, y: isize| -> bool {
        if x < 0 || y < 0 || (x as usize) >= width || (y as usize) >= height {
            return true;
        }
        map.is_wall(x as usize, y as usize)
    };

    // Concavity-only AO: darken inward corners, keep straight/outward corners unmodified.
    let corner_light = |side_a: bool, side_b: bool, _diag: bool| -> u8 {
        if side_a && side_b {
            // Both adjacent sides are walls — this is a concave nook corner.
            let light = (1.0 - strength).max(min_light);
            (light * 255.0).round() as u8
        } else {
            255
        }
    };

    for y in 0..height {
        for x in 0..width {
            let xi = x as isize;
            let yi = y as isize;

            let tl = corner_light(
                wall_at(xi - 1, yi), // left
                wall_at(xi, yi - 1), // up
                wall_at(xi - 1, yi - 1), // up-left
            );

            let tr = corner_light(
                wall_at(xi + 1, yi), // right
                wall_at(xi, yi - 1), // up
                wall_at(xi + 1, yi - 1), // up-right
            );

            let br = corner_light(
                wall_at(xi + 1, yi), // right
                wall_at(xi, yi + 1), // down
                wall_at(xi + 1, yi + 1), // down-right
            );

            let bl = corner_light(
                wall_at(xi - 1, yi), // left
                wall_at(xi, yi + 1), // down
                wall_at(xi - 1, yi + 1), // down-left
            );

            corners[y * width + x] = [tl, tr, br, bl];
        }
    }

    AoField {
        width,
        height,
        corners,
    }
}

pub fn rebuild_ao(ao: &mut AoField, map: &Map, params: &AoParameters) {
    *ao = build_ao(map, params);
}

pub fn load_map(file_path: &str) -> Map {
    let img: RgbImage = image::open(file_path)
        .expect("Failed to open map image")
        .to_rgb8();

    let mut tiles: Vec<Vec<u8>> = Vec::new();

    for y in 0..img.height() {
        let mut row: Vec<u8> = Vec::new();
        for x in 0..img.width() {
            let pixel = img.get_pixel(x, y);
            let color_value = (pixel[0] as u32) << 16 | (pixel[1] as u32) << 8 | (pixel[2] as u32);

            let cell_value = match color_value {
                x if x == ColorMap::Wall1Colour as u32 => 1,
                x if x == ColorMap::Wall2Colour as u32 => 2,
                x if x == ColorMap::Wall3Color as u32 => 3,
                _ => 0, // Default to empty space
            };
            row.push(cell_value);
        }
        tiles.push(row);
    }

    Map::new(tiles)
}
