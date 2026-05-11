#![allow(unused)]
use std::sync::Arc;

use super::Controller;
use crate::input::InputMessage;
use crate::model::{ControllerId, Level, Waypoint};
use crate::net::bots::{manhattan_distance, random_empty_waypoint};
use crate::simulation::GameState;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct WanderingController {
    pub id: ControllerId,
    pub target_tile: Option<Waypoint>,
    pub path: Option<Vec<Waypoint>>,
    pub last_state: Option<GameState>,
    pub current_tick: u64,
    pub level: Arc<Level>,
    current_waypoint: usize,
    wait_ticks_remaining: u32,
}

impl WanderingController {
    pub fn new(id: ControllerId, level: Arc<Level>) -> Self {
        Self {
            id,
            target_tile: None,
            path: None,
            last_state: None,
            current_tick: 0,
            level,
            current_waypoint: 0,
            wait_ticks_remaining: 0,
        }
    }

    pub fn set_target(&mut self, target: Waypoint) {
        self.target_tile = Some(target);
        self.path = None;
        self.current_waypoint = 0;
    }

    fn steer(&mut self) -> f64 {
        let state = match &self.last_state {
            Some(s) => s,
            None => return 0.0,
        };
        let pawn = match state.controlled_entity(self.id) {
            Some(pawn) => pawn,
            None => return 0.0,
        };
        let path = match &self.path {
            Some(p) if !p.is_empty() => p,
            _ => return 0.0,
        };

        let waypoint = &path[self.current_waypoint];
        let dx = waypoint.x - pawn.x;
        let dy = waypoint.y - pawn.y;
        let dist = (dx * dx + dy * dy).sqrt();

        // advance to next waypoint when close enough
        if dist < 1.0 && self.current_waypoint + 1 < path.len() {
            self.current_waypoint += 1;
        } else if dist < 1.0 {
            self.path = None;
            self.target_tile = None;
            self.wait_ticks_remaining = rand::random_range(60..=180);
            return 0.0;
        }

        // cross product of current direction vs desired direction gives signed turn
        let desired_x = dx / dist.max(1e-6);
        let desired_y = dy / dist.max(1e-6);
        let player = match state.players.get(&self.id) {
            Some(player) => player,
            None => return 0.0,
        };
        let cross = player.dir_x * desired_y - player.dir_y * desired_x;
        (cross * 0.1).clamp(-1.0, 1.0)
    }

    pub fn compute_path(&mut self) {
        let state = match &self.last_state {
            Some(s) => s,
            None => return,
        };
        let pawn = match state.controlled_entity(self.id) {
            Some(a) => a,
            None => return,
        };
        let target = match &self.target_tile {
            Some(t) => t,
            None => return,
        };

        let start = (pawn.x as i32, pawn.y as i32);
        let goal = (target.x as i32, target.y as i32);

        let mut open_set = BinaryHeap::<Reverse<(u32, (i32, i32))>>::new();
        let mut came_from = std::collections::HashMap::<(i32, i32), (i32, i32)>::new();
        let mut g_costs = std::collections::HashMap::<(i32, i32), u32>::new();

        g_costs.insert(start, 0);
        open_set.push(Reverse((0, start)));

        while let Some(Reverse((_, current))) = open_set.pop() {
            if current == goal {
                // reconstruct path including goal
                let mut path = Vec::new();
                let mut node = current;
                loop {
                    path.push(Waypoint::new(node.0 as f64 + 0.5, node.1 as f64 + 0.5));
                    match came_from.get(&node) {
                        Some(&prev) => node = prev,
                        None => break, // reached start
                    }
                }
                path.reverse();
                self.path = Some(path);
                return;
            }

            let current_g = *g_costs.get(&current).unwrap_or(&u32::MAX);

            for neighbor in &[
                (current.0 + 1, current.1),
                (current.0 - 1, current.1),
                (current.0, current.1 + 1),
                (current.0, current.1 - 1),
            ] {
                if self.level.is_wall(neighbor.0 as usize, neighbor.1 as usize) {
                    continue;
                }
                let tentative_g = current_g + 1;
                if tentative_g < *g_costs.get(neighbor).unwrap_or(&u32::MAX) {
                    g_costs.insert(*neighbor, tentative_g);
                    came_from.insert(*neighbor, current);
                    let h = manhattan_distance(*neighbor, goal);
                    open_set.push(Reverse((tentative_g + h, *neighbor)));
                }
            }
        }
    }
}

impl Controller for WanderingController {
    fn id(&self) -> ControllerId {
        self.id
    }

    fn poll_inputs(&mut self) -> Vec<InputMessage> {
        self.current_tick += 1;

        if self.path.is_none() && self.target_tile.is_none() {
            if self.wait_ticks_remaining > 0 {
                self.wait_ticks_remaining -= 1;
            } else {
                self.target_tile = random_empty_waypoint(&self.level);
                self.compute_path();
                self.current_waypoint = 0;
            }
        }

        let rotate_delta = self.steer();
        vec![InputMessage {
            controller_id: self.id,
            tick: self.current_tick,
            forward: self.path.is_some(),
            rotate_delta,
            ..Default::default()
        }]
    }

    fn receive_state(&mut self, state: &GameState) {
        self.last_state = Some(state.clone());
    }
}
