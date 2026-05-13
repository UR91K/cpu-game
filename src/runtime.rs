use std::sync::Arc;
use std::sync::mpsc::Receiver;

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

#[derive(Clone, Debug)]
pub struct AuthoritativeUpdate {
    pub snapshot: ClientSnapshot,
}

impl AuthoritativeUpdate {
    pub fn from_game_state(game_state: GameState) -> Self {
        Self {
            snapshot: ClientSnapshot::from_game_state(game_state),
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

    pub fn update_snapshot(&mut self, snapshot: Option<ClientSnapshot>) {
        self.snapshot = snapshot;
    }

    pub fn apply_update(&mut self, update: AuthoritativeUpdate) {
        self.update_snapshot(Some(update.snapshot));
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

#[allow(dead_code)]
pub struct ChannelClientRuntime {
    updates: Receiver<AuthoritativeUpdate>,
    client: SnapshotRuntime,
}

#[allow(dead_code)]
impl ChannelClientRuntime {
    pub fn new(level: Arc<Level>, updates: Receiver<AuthoritativeUpdate>) -> Self {
        Self {
            updates,
            client: SnapshotRuntime::new(level),
        }
    }

    fn drain_updates(&mut self) {
        while let Ok(update) = self.updates.try_recv() {
            self.client.apply_update(update);
        }
    }
}

impl GameRuntime for ChannelClientRuntime {
    fn advance(&mut self, _frame_dt: f64) {
        self.drain_updates();
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