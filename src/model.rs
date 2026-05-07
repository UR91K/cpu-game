
use crate::texture::{AnimationStyle, FacingMode, VisualId};

pub type PlayerId = u64;
pub type ObjectId = u64;

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
pub enum ObjectKind {
    Actor { owner_player: Option<PlayerId> },
    StaticProp { blocks_movement: bool },
    Pickup { pickup_kind: PickupKind },
    Projectile {
        owner_player: Option<PlayerId>,
        ttl_ticks: u32,
        damage: u32,
    },
}

#[derive(Clone, Debug)]
pub struct WorldObject {
    pub id: ObjectId,
    pub x: f64,
    pub y: f64,
    pub vel_x: f64,
    pub vel_y: f64,
    pub radius: f64,
    pub render: Option<RenderBody>,
    pub kind: ObjectKind,
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