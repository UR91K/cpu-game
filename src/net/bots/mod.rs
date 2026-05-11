use std::sync::Arc;

use super::Controller;
use crate::input::InputMessage;
use crate::model::{ControllerId, Level, Waypoint};
use crate::simulation::GameState;
use rand::rng;
use rand::seq::{IndexedRandom, SliceRandom};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub mod wandering;
pub mod waypoint;

pub fn manhattan_distance(a: (i32, i32), b: (i32, i32)) -> u32 {
    ((a.0 - b.0).abs() + (a.1 - b.1).abs()) as u32
}

pub fn random_empty_waypoint(level: &Level) -> Option<Waypoint> {
    let empty_tiles = level.get_empty_tiles();
    if empty_tiles.is_empty() {
        return None;
    }
    let (x, y) = empty_tiles.choose(&mut rand::rng())?;
    Some(Waypoint::new(*x as f64 + 0.5, *y as f64 + 0.5))
}
