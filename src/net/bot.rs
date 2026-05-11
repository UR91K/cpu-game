use std::sync::Arc;

use crate::input::InputMessage;
use crate::model::{Map, PlayerId};
use crate::simulation::GameState;
use super::Client;
use std::collections::BinaryHeap;
use std::cmp::Reverse;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::rng;

pub struct WaypointBot {
    pub id: PlayerId,
    waypoints: Vec<Waypoint>,
    current_waypoint: usize,
    last_state: Option<GameState>,
    current_tick: u64,
}

pub struct Waypoint {
    pub x: f64,
    pub y: f64,
}

impl Waypoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn is_wall(&self, map: &Map) -> bool {
        map.is_wall(self.x as usize, self.y as usize)
    }
}

impl WaypointBot {
    pub fn new(id: PlayerId, waypoints: Vec<Waypoint>) -> Self {
        Self {
            id,
            waypoints,
            current_waypoint: 0,
            last_state: None,
            current_tick: 0,
        }
    }

    /// get an angle to steer toward the next waypoint
    fn steer(&mut self) -> f64 {
        let state = match &self.last_state {
            Some(s) => s,
            None => return 0.0,
        };
        let actor = match state.controlled_object(self.id) {
            Some(actor) => actor,
            None => return 0.0,
        };

        if self.waypoints.is_empty() {
            return 0.0;
        }

        let waypoint = &self.waypoints[self.current_waypoint];
        let dx = waypoint.x - actor.x;
        let dy = waypoint.y - actor.y;
        let dist = (dx * dx + dy * dy).sqrt();

        // advance to next waypoint when close enough
        if dist < 0.5 && self.waypoints.len() > 1 {
            self.current_waypoint = (self.current_waypoint + 1) % self.waypoints.len();
        }

        // cross product of current direction vs desired direction gives signed turn
        let desired_x = dx / dist.max(1e-6);
        let desired_y = dy / dist.max(1e-6);
        let player = match state.players.get(&self.id) {
            Some(player) => player,
            None => return 0.0,
        };
        let cross = player.dir_x * desired_y - player.dir_y * desired_x;
        cross * 0.05
    }
}

impl Client for WaypointBot {
    fn id(&self) -> PlayerId {
        self.id
    }

    fn poll_inputs(&mut self) -> Vec<InputMessage> {
        self.current_tick += 1;
        let rotate_delta = self.steer();
        vec![InputMessage {
            player_id: self.id,
            tick: self.current_tick,
            forward: true,
            rotate_delta,
            ..Default::default()
        }]
    }

    fn receive_state(&mut self, state: &GameState) {
        self.last_state = Some(state.clone());
    }
}

pub struct AStarBot {
    pub id: PlayerId,
    pub target_tile: Option<Waypoint>,
    pub path: Option<Vec<Waypoint>>,
    pub last_state: Option<GameState>,
    pub current_tick: u64,
    pub map: Arc<Map>,
    current_waypoint: usize,
    wait_ticks_remaining: u32,
}

impl AStarBot {
    pub fn new(id: PlayerId, map: Arc<Map>) -> Self {
        Self {
            id,
            target_tile: None,
            path: None,
            last_state: None,
            current_tick: 0,
            map,
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
        let actor = match state.controlled_object(self.id) {
            Some(actor) => actor,
            None => return 0.0,
        };
        let path = match &self.path {
            Some(p) if !p.is_empty() => p,
            _ => return 0.0,
        };

        let waypoint = &path[self.current_waypoint];
        let dx = waypoint.x - actor.x;
        let dy = waypoint.y - actor.y;
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
        let state = match &self.last_state { Some(s) => s, None => return };
        let actor = match state.controlled_object(self.id) { Some(a) => a, None => return };
        let target = match &self.target_tile { Some(t) => t, None => return };

        let start = (actor.x as i32, actor.y as i32);
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
                if self.map.is_wall(neighbor.0 as usize, neighbor.1 as usize) {
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

impl Client for AStarBot {
    fn id(&self) -> PlayerId {
        self.id
    }

    fn poll_inputs(&mut self) -> Vec<InputMessage> {
        self.current_tick += 1;

        if self.path.is_none() && self.target_tile.is_none() {
            if self.wait_ticks_remaining > 0 {
                self.wait_ticks_remaining -= 1;
            } else {
                self.target_tile = random_empty_waypoint(&self.map);
                self.compute_path();
                self.current_waypoint = 0;
            }
        }

        let rotate_delta = self.steer();
        vec![InputMessage {
            player_id: self.id,
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

pub fn manhattan_distance(a: (i32, i32), b: (i32, i32)) -> u32 {
    ((a.0 - b.0).abs() + (a.1 - b.1).abs()) as u32
}

pub fn random_empty_waypoint(map: &Map) -> Option<Waypoint> {
    let empty_tiles = map.get_empty_tiles();
    if empty_tiles.is_empty() {
        return None;
    }
    let (x, y) = empty_tiles.choose(&mut rand::rng())?;
    Some(Waypoint::new(*x as f64 + 0.5, *y as f64 + 0.5))
}