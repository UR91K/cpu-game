
pub type PlayerId = u64;

#[derive(Clone, Debug)]
pub struct Sprite {
    pub x: f64,
    pub y: f64,
    pub texture_index: usize,
    pub movement_angle: f64,
    pub is_moving: bool,
}

#[derive(Clone, Debug)]
pub struct Map {
    pub tiles: Vec<Vec<u8>>,
}

impl Map {
    pub fn new(tiles: Vec<Vec<u8>>) -> Self {
        Self { tiles }
    }

    pub fn is_wall(&self, x: usize, y: usize) -> bool {
        self.tiles[y][x] > 0
    }

    pub fn tile_at(&self, x: usize, y: usize) -> u8 {
        self.tiles[y][x]
    }
}

pub struct AoField {
    pub width: usize,
    pub height: usize,
    pub corners: Vec<[u8; 4]>,
}

pub struct AoParameters {
    pub corner_strength: f64,
    pub wall_seam_strength: f64,
    pub minimum_light: f64,
}