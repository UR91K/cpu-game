use crate::clock::ClockManager;
use crate::model::{EntityId, Level};
use crate::simulation::GameState;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct SoundEvent {
    pub entity_id: EntityId,
    pub kind: SoundEventKind,
    pub x: f64,
    pub y: f64,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub enum SoundEventKind {
    Footstep,
    Gunshot,
    Pickup,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ClientSnapshot {
    pub game_state: GameState,
    pub authoritative_tick: u64,
    pub sound_events: Vec<SoundEvent>,
}

impl ClientSnapshot {
    pub fn from_game_state(game_state: GameState) -> Self {
        let authoritative_tick = game_state.tick;
        Self {
            game_state,
            authoritative_tick,
            sound_events: Vec::new(),
        }
    }
}

pub trait GameRuntime {
    fn advance(&mut self, frame_dt: f64);
    fn snapshot(&self) -> Option<ClientSnapshot>;
    fn level(&self) -> &Level;
}

impl GameRuntime for ClockManager {
    fn advance(&mut self, frame_dt: f64) {
        ClockManager::advance(self, frame_dt);
    }

    fn snapshot(&self) -> Option<ClientSnapshot> {
        self.server_state()
            .cloned()
            .map(ClientSnapshot::from_game_state)
    }

    fn level(&self) -> &Level {
        ClockManager::level(self)
    }
}