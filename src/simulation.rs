use std::collections::HashMap;

use crate::input::InputMessage;
use crate::model::{Map, PlayerId, Sprite};

pub const TICK_RATE: u64 = 64;
pub const TICK_DT: f64 = 1.0 / TICK_RATE as f64;
pub const MOVE_SPEED: f64 = 40.0;
pub const FRICTION: f64 = 10.0;
const PLAYER_SPRITE_TEXTURE_INDEX: usize = 3;

#[derive(Clone, Debug)]
pub struct PlayerState {
    pub x: f64,
    pub y: f64,
    pub dir_x: f64,
    pub dir_y: f64,
    pub plane_x: f64,
    pub plane_y: f64,
    pub vel_x: f64,
    pub vel_y: f64,
}

impl PlayerState {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            dir_x: -1.0,
            dir_y: 0.0,
            plane_x: 0.0,
            plane_y: 0.66,
            vel_x: 0.0,
            vel_y: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GameState {
    pub players: HashMap<PlayerId, PlayerState>,
    pub sprites: Vec<Sprite>,
    pub tick: u64,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            sprites: Vec::new(),
            tick: 0,
        }
    }
}

/// pure function to advance the simulation by applying inputs to the given state
/// both clients and server can use this to stay in sync
pub fn tick(state: &GameState, inputs: &[InputMessage], map: &Map, delta: f64) -> GameState {
    let mut next = state.clone();
    for msg in inputs {
        apply_input(&mut next, msg, map, delta);
    }
    next.sprites = next
        .players
        .values()
        .map(|player| Sprite {
            x: player.x,
            y: player.y,
            texture_index: PLAYER_SPRITE_TEXTURE_INDEX,
            movement_angle: {
                let speed_sq = player.vel_x * player.vel_x + player.vel_y * player.vel_y;
                if speed_sq > 1e-6 {
                    player.vel_y.atan2(player.vel_x)
                } else {
                    player.dir_y.atan2(player.dir_x)
                }
            },
            is_moving: player.vel_x * player.vel_x + player.vel_y * player.vel_y > 1e-6,
        })
        .collect();
    next.tick += 1;
    next
}

pub fn apply_input(state: &mut GameState, input: &InputMessage, map: &Map, delta: f64) {
    let Some(player) = state.players.get_mut(&input.player_id) else {
        return;
    };

    // rotation 
    // apply before movement so that movement is based on the new direction immediately
    if input.rotate_delta != 0.0 {
        let angle = input.rotate_delta;
        let (sin, cos) = angle.sin_cos();
        let old_dir_x = player.dir_x;
        player.dir_x = old_dir_x * cos - player.dir_y * sin;
        player.dir_y = old_dir_x * sin + player.dir_y * cos;
        let old_plane_x = player.plane_x;
        player.plane_x = old_plane_x * cos - player.plane_y * sin;
        player.plane_y = old_plane_x * sin + player.plane_y * cos;
    }

    // acceleration
    let mut move_dir_x = 0.0f64;
    let mut move_dir_y = 0.0f64;
    if input.forward {
        move_dir_x += player.dir_x;
        move_dir_y += player.dir_y;
    }
    if input.back {
        move_dir_x -= player.dir_x;
        move_dir_y -= player.dir_y;
    }
    if input.strafe_left {
        move_dir_x -= player.plane_x;
        move_dir_y -= player.plane_y;
    }
    if input.strafe_right {
        move_dir_x += player.plane_x;
        move_dir_y += player.plane_y;
    }
    player.vel_x += move_dir_x * MOVE_SPEED * delta;
    player.vel_y += move_dir_y * MOVE_SPEED * delta;

    // friction
    let speed_sq = player.vel_x * player.vel_x + player.vel_y * player.vel_y;
    if speed_sq > 0.0 {
        let speed = speed_sq.sqrt();
        let drop = speed * FRICTION * delta;
        let new_speed = (speed - drop).max(0.0);
        if new_speed < speed {
            player.vel_x *= new_speed / speed;
            player.vel_y *= new_speed / speed;
        }
    }

    // actually move + collide with walls
    let dx = player.vel_x * delta;
    let dy = player.vel_y * delta;
    if !map.is_wall((player.x + dx) as usize, player.y as usize) {
        player.x += dx;
    } else {
        player.vel_x = 0.0;
    }
    if !map.is_wall(player.x as usize, (player.y + dy) as usize) {
        player.y += dy;
    } else {
        player.vel_y = 0.0;
    }
}
