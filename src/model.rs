
pub type PlayerId = u64;

#[derive(Clone, Debug)]
pub struct Sprite {
    pub x: f64,
    pub y: f64,
    pub texture_index: usize,
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
        self.tiles[x][y] > 0
    }

    pub fn tile_at(&self, x: usize, y: usize) -> u8 {
        self.tiles[x][y]
    }
}