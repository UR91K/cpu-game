use crate::input::InputMessage;
use crate::model::PlayerId;
use crate::simulation::GameState;
use super::Client;

pub struct BotClient {
    pub id: PlayerId,
    waypoints: Vec<(f64, f64)>,
    current_waypoint: usize,
    last_state: Option<GameState>,
    current_tick: u64,
}

impl BotClient {
    pub fn new(id: PlayerId, waypoints: Vec<(f64, f64)>) -> Self {
        Self {
            id,
            waypoints,
            current_waypoint: 0,
            last_state: None,
            current_tick: 0,
        }
    }

    /// Returns a rotation angle (radians) to steer toward the next waypoint.
    fn steer(&mut self) -> f64 {
        let state = match &self.last_state {
            Some(s) => s,
            None => return 0.0,
        };
        let player = match state.players.get(&self.id) {
            Some(p) => p,
            None => return 0.0,
        };

        if self.waypoints.is_empty() {
            return 0.0;
        }

        let (wx, wy) = self.waypoints[self.current_waypoint];
        let dx = wx - player.x;
        let dy = wy - player.y;
        let dist = (dx * dx + dy * dy).sqrt();

        // Advance to next waypoint when close enough
        if dist < 0.5 && self.waypoints.len() > 1 {
            self.current_waypoint = (self.current_waypoint + 1) % self.waypoints.len();
        }

        // Cross product of current direction vs desired direction gives signed turn
        let desired_x = dx / dist.max(1e-6);
        let desired_y = dy / dist.max(1e-6);
        let cross = player.dir_x * desired_y - player.dir_y * desired_x;
        cross * 0.05
    }
}

impl Client for BotClient {
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
