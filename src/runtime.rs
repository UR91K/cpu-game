use crate::clock::ClockManager;
use crate::model::Level;
use crate::simulation::GameState;

pub trait GameRuntime {
    fn advance(&mut self, frame_dt: f64);
    fn snapshot(&self) -> Option<GameState>;
    fn level(&self) -> &Level;
}

impl GameRuntime for ClockManager {
    fn advance(&mut self, frame_dt: f64) {
        ClockManager::advance(self, frame_dt);
    }

    fn snapshot(&self) -> Option<GameState> {
        self.server_state().cloned()
    }

    fn level(&self) -> &Level {
        ClockManager::level(self)
    }
}