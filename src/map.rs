use core::panic;

use include_dir::{include_dir, Dir};

use crate::model::Map;
use crate::texture::FloorTexture;

const MAGIC: &[u8; 4] = b"AMAP";
const VERSION: u16 = 1;
const HEADER_SIZE: usize = 32;
const BYTES_PER_TILE: usize = 8;

static TEXTURES_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/textures");

pub fn load_embedded_map() -> Map {
    let map_file = TEXTURES_DIR
        .get_file("map.amap")
        .unwrap_or_else(|| panic!("Failed to find embedded map.amap"));

    map_from_binary(map_file.contents())
        .unwrap_or_else(|e| panic!("Failed to decode embedded map.amap: {e}"))
}

fn map_from_binary(data: &[u8]) -> Result<Map, String> {
    if data.len() < HEADER_SIZE {
        return Err(format!("File too small: {} bytes", data.len()));
    }
    if &data[0..4] != MAGIC {
        return Err("Invalid magic bytes".into());
    }

    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != VERSION {
        return Err(format!("Unsupported version: {version}"));
    }

    let width  = u16::from_le_bytes([data[6],  data[7]])  as usize;
    let height = u16::from_le_bytes([data[8],  data[9]])  as usize;

    let required = HEADER_SIZE + width * height * BYTES_PER_TILE;
    if data.len() < required {
        return Err(format!("File too small for {width}×{height} map: need {required}, got {}", data.len()));
    }

    let mut tiles:       Vec<Vec<u8>>          = Vec::with_capacity(height);
    let mut floor_tiles: Vec<Vec<FloorTexture>> = Vec::with_capacity(height);

    for y in 0..height {
        let mut row       = Vec::with_capacity(width);
        let mut floor_row = Vec::with_capacity(width);

        for x in 0..width {
            let base = HEADER_SIZE + (y * width + x) * BYTES_PER_TILE;

            // Byte layout (8 bytes per tile):
            //   [0] wall_type      — 0 = floor, 1-254 = wall variants, 255 = void
            //   [1] floor_texture  — only meaningful when wall_type == 0
            //   [2] wall_height    — 0 = default
            //   [3] ceiling_texture
            //   [4] prop_type
            //   [5] prop_variant
            //   [6] special_type   — 0 = none, 1 = spawn, 2-3 = teleporter, 4 = trigger
            //   [7] special_data   — destination ID or trigger ID
            let wall_type     = data[base];
            let floor_texture = data[base + 1];
            let _wall_height  = data[base + 2]; // wire into Map when ready
            let _prop_type    = data[base + 4]; // wire into Map when ready
            let _special_type = data[base + 6]; // wire into Map when ready
            let _special_data = data[base + 7]; // wire into Map when ready

            row.push(wall_type);
            floor_row.push(FloorTexture::from_u8(floor_texture).unwrap_or(FloorTexture::Smooth));
        }

        tiles.push(row);
        floor_tiles.push(floor_row);
    }

    Ok(Map::with_floor_tiles(tiles, floor_tiles))
}
