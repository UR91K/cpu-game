use super::Controller;
use crate::input::InputMessage;
use crate::model::{ControllerId, Waypoint};
use crate::simulation::GameState;

pub struct WaypointController {
    pub id: ControllerId,
    waypoints: Vec<Waypoint>,
    current_waypoint: usize,
    last_state: Option<GameState>,
    current_tick: u64,
}

impl WaypointController {
    pub fn new(id: ControllerId, waypoints: Vec<Waypoint>) -> Self {
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
        let pawn = match state.controlled_entity(self.id) {
            Some(pawn) => pawn,
            None => return 0.0,
        };

        if self.waypoints.is_empty() {
            return 0.0;
        }

        let waypoint = &self.waypoints[self.current_waypoint];
        let dx = waypoint.x - pawn.x;
        let dy = waypoint.y - pawn.y;
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

impl Controller for WaypointController {
    fn id(&self) -> ControllerId {
        self.id
    }

    fn poll_inputs(&mut self) -> Vec<InputMessage> {
        self.current_tick += 1;
        let rotate_delta = self.steer();
        vec![InputMessage {
            controller_id: self.id,
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
