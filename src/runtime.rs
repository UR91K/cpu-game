use crate::clock::ClockManager;
use crate::model::Level;
use crate::simulation::GameState;

pub trait GameRuntime {
    fn advance(&mut self, frame_dt: f64);
    fn game_state(&self) -> Option<&GameState>;
    fn level(&self) -> &Level;
}

impl GameRuntime for ClockManager {
    fn advance(&mut self, frame_dt: f64) {
        ClockManager::advance(self, frame_dt);
    }

    fn game_state(&self) -> Option<&GameState> {
        self.server_state()
    }

    fn level(&self) -> &Level {
        ClockManager::level(self)
    }
}