use crate::texture::{AnimationStyle, FacingMode, FloorTexture, VisualId};

pub type ControllerId = u64;
pub type EntityId = u64;

#[derive(Clone, Debug)]
pub struct RenderBody {
    pub visual: VisualId,
    pub width: f32,
    pub height: f32,
    pub facing_mode: FacingMode,
    pub animation: AnimationStyle,
}

#[derive(Clone, Debug)]
pub enum PickupKind {
    Medkit,
}

#[derive(Clone, Debug)]
pub enum EntityKind {
    Pawn {
        owner_id: Option<ControllerId>,
    },
    StaticProp {
        blocks_movement: bool,
    },
    Pickup {
        pickup_kind: PickupKind,
    },
    Projectile {
        owner_id: Option<ControllerId>,
        ttl_ticks: u32,
        damage: u32,
    },
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub id: EntityId,
    pub x: f64,
    pub y: f64,
    pub vel_x: f64,
    pub vel_y: f64,
    pub radius: f64,
    pub render: Option<RenderBody>,
    pub kind: EntityKind,
}

#[derive(Clone, Debug)]
pub struct Level {
    pub tiles: Vec<Vec<u8>>,
    pub floor_tiles: Vec<Vec<FloorTexture>>,
}

impl Level {
    pub fn new(tiles: Vec<Vec<u8>>) -> Self {
        let height = tiles.len();
        let width = tiles.first().map_or(0, |row| row.len());
        let floor_tiles = vec![vec![FloorTexture::Smooth; width]; height];
        Self { tiles, floor_tiles }
    }

    pub fn with_floor_tiles(tiles: Vec<Vec<u8>>, floor_tiles: Vec<Vec<FloorTexture>>) -> Self {
        assert_eq!(
            tiles.len(),
            floor_tiles.len(),
            "floor tile row count must match level tiles"
        );
        for (tile_row, floor_row) in tiles.iter().zip(floor_tiles.iter()) {
            assert_eq!(
                tile_row.len(),
                floor_row.len(),
                "floor tile column count must match level tiles"
            );
        }
        Self { tiles, floor_tiles }
    }

    pub fn is_wall(&self, x: usize, y: usize) -> bool {
        self.tiles[y][x] > 0
    }

    pub fn tile_at(&self, x: usize, y: usize) -> u8 {
        self.tiles[y][x]
    }

    pub fn floor_at(&self, x: usize, y: usize) -> FloorTexture {
        self.floor_tiles[y][x]
    }

    pub fn get_empty_tiles(&self) -> Vec<(usize, usize)> {
        let mut empty_tiles = Vec::new();
        for (y, row) in self.tiles.iter().enumerate() {
            for (x, &tile) in row.iter().enumerate() {
                if tile == 0 {
                    empty_tiles.push((x, y));
                }
            }
        }
        empty_tiles
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

pub struct Waypoint {
    pub x: f64,
    pub y: f64,
}

impl Waypoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn is_wall(&self, level: &Level) -> bool {
        level.is_wall(self.x as usize, self.y as usize)
    }
}
