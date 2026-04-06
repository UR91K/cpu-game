use std::collections::HashMap;

use crate::model::{InputMessage, Map, PlayerId, Sprite, TICK_DT};

#[derive(Clone, Debug)]
pub struct GameState {
    pub players: HashMap<PlayerId, PlayerState>,
    pub sprites: Vec<Sprite>,
    pub tick: u64,
}

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

pub fn tick(state: &GameState, inputs: &[InputMessage], map: &Map) -> GameState {
    let mut next = state.clone();
    for msg in inputs {
        apply_input(&mut next, msg, map, TICK_DT);
    }
    next.tick += 1;
    next
}

pub fn apply_input(state: &mut GameState, input: &InputMessage, map: &Map, delta: f64) {
    todo!()
}
