use std::sync::Arc;

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

pub struct SnapshotRuntime {
    level: Arc<Level>,
    snapshot: Option<ClientSnapshot>,
}

impl SnapshotRuntime {
    pub fn new(level: Arc<Level>) -> Self {
        Self {
            level,
            snapshot: None,
        }
    }

    pub fn with_snapshot(level: Arc<Level>, snapshot: Option<ClientSnapshot>) -> Self {
        let mut runtime = Self::new(level);
        runtime.update_snapshot(snapshot);
        runtime
    }

    pub fn update_snapshot(&mut self, snapshot: Option<ClientSnapshot>) {
        self.snapshot = snapshot;
    }
}

impl GameRuntime for SnapshotRuntime {
    fn advance(&mut self, _frame_dt: f64) {}

    fn snapshot(&self) -> Option<ClientSnapshot> {
        self.snapshot.clone()
    }

    fn level(&self) -> &Level {
        self.level.as_ref()
    }
}

pub struct LocalClientRuntime {
    authority: ClockManager,
    client: SnapshotRuntime,
}

impl LocalClientRuntime {
    pub fn new(authority: ClockManager) -> Self {
        let level = authority.level_arc();
        let snapshot = authority
            .server_state()
            .cloned()
            .map(ClientSnapshot::from_game_state);
        Self {
            authority,
            client: SnapshotRuntime::with_snapshot(level, snapshot),
        }
    }

    fn refresh_snapshot(&mut self) {
        let snapshot = self
            .authority
            .server_state()
            .cloned()
            .map(ClientSnapshot::from_game_state);
        self.client.update_snapshot(snapshot);
    }
}

impl GameRuntime for LocalClientRuntime {
    fn advance(&mut self, frame_dt: f64) {
        self.authority.advance(frame_dt);
        self.refresh_snapshot();
    }

    fn snapshot(&self) -> Option<ClientSnapshot> {
        self.client.snapshot()
    }

    fn level(&self) -> &Level {
        self.client.level()
    }
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