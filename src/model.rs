
pub const TICK_RATE: u64 = 64;
pub const TICK_DT: f64 = 1.0 / TICK_RATE as f64;

#[derive(Clone, Debug)]
pub struct Player {
    pub x: f64,
    pub y: f64,
    pub dir_x: f64,
    pub dir_y: f64,
    pub plane_x: f64,
    pub plane_y: f64,
    pub move_speed: f64,
    pub vel_x: f64,
    pub vel_y: f64,
    pub friction: f64,
}

#[derive(Clone, Debug)]
pub struct Sprite {
    pub x: f64,
    pub y: f64,
    pub texture_index: usize,
}

pub type PlayerId = u64;

#[derive(Clone, Debug)]
pub struct InputMessage {
    pub player_id: PlayerId,
    pub tick: u64,
    pub action: InputAction,
}

#[derive(Clone, Debug)]
pub enum InputAction {
    MoveForward,
    MoveBackward,
    StrafeLeft,
    StrafeRight,
    RotationAngle(f64),
    Fire,
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