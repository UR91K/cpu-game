use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use crate::clock::ClockManager;
use crate::input::InputMessage;
use crate::model::{ControllerId, EntityId, Level};
use crate::simulation::{GameState, TICK_DT};
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SoundEvent {
    pub entity_id: EntityId,
    pub kind: SoundEventKind,
    pub x: f64,
    pub y: f64,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SoundEventKind {
    Footstep,
    Gunshot,
    Pickup,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransportDebug {
    pub received_count: u64,
    pub polled_count: u64,
    pub last_received_input: Option<InputMessage>,
    pub last_polled_input: Option<InputMessage>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientSnapshot {
    pub game_state: GameState,
    pub authoritative_tick: u64,
    pub local_controller_id: Option<ControllerId>,
    pub sound_events: Vec<SoundEvent>,
    pub transport_debug: Option<TransportDebug>,
}

impl ClientSnapshot {
    pub fn from_game_state(
        game_state: GameState,
        local_controller_id: Option<ControllerId>,
        transport_debug: Option<TransportDebug>,
    ) -> Self {
        let authoritative_tick = game_state.tick;
        Self {
            game_state,
            authoritative_tick,
            local_controller_id,
            sound_events: Vec::new(),
            transport_debug,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthoritativeUpdate {
    pub snapshot: ClientSnapshot,
}

impl AuthoritativeUpdate {
    pub fn from_game_state(
        game_state: GameState,
        local_controller_id: Option<ControllerId>,
        transport_debug: Option<TransportDebug>,
    ) -> Self {
        Self {
            snapshot: ClientSnapshot::from_game_state(game_state, local_controller_id, transport_debug),
        }
    }
}

pub trait GameRuntime {
    fn advance(&mut self, frame_dt: f64);
    fn snapshot(&self) -> Option<ClientSnapshot>;
    fn local_controller_id(&self) -> Option<ControllerId>;
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

    fn local_controller_id(&self) -> Option<ControllerId> {
        self.snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.local_controller_id)
    }

    fn level(&self) -> &Level {
        self.level.as_ref()
    }
}

#[allow(dead_code)]
pub struct ChannelClientRuntime {
    updates: Receiver<AuthoritativeUpdate>,
    client: SnapshotRuntime,
    previous_snapshot: Option<ClientSnapshot>,
    last_update_at: Option<Instant>,
}

#[allow(dead_code)]
impl ChannelClientRuntime {
    pub fn new(level: Arc<Level>, updates: Receiver<AuthoritativeUpdate>) -> Self {
        Self {
            updates,
            client: SnapshotRuntime::new(level),
            previous_snapshot: None,
            last_update_at: None,
        }
    }

    fn drain_updates(&mut self) {
        while let Ok(update) = self.updates.try_recv() {
            self.previous_snapshot = self.client.snapshot();
            self.client.apply_update(update);
            self.last_update_at = Some(Instant::now());
        }
    }

    fn interpolated_snapshot(&self) -> Option<ClientSnapshot> {
        let current = self.client.snapshot()?;
        let previous = self.previous_snapshot.as_ref()?;

        if current.local_controller_id != previous.local_controller_id {
            return Some(current);
        }

        if current.authoritative_tick <= previous.authoritative_tick {
            return Some(current);
        }

        let Some(last_update_at) = self.last_update_at else {
            return Some(current);
        };

        let alpha = (last_update_at.elapsed().as_secs_f64() / TICK_DT).clamp(0.0, 1.0);
        Some(interpolate_snapshot(previous, &current, alpha))
    }
}

impl GameRuntime for ChannelClientRuntime {
    fn advance(&mut self, _frame_dt: f64) {
        self.drain_updates();
    }

    fn snapshot(&self) -> Option<ClientSnapshot> {
        self.interpolated_snapshot().or_else(|| self.client.snapshot())
    }

    fn local_controller_id(&self) -> Option<ControllerId> {
        self.client.local_controller_id()
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
            .map(|game_state| ClientSnapshot::from_game_state(game_state, None, None))
    }

    fn local_controller_id(&self) -> Option<ControllerId> {
        None
    }

    fn level(&self) -> &Level {
        ClockManager::level(self)
    }
}

fn interpolate_snapshot(
    previous: &ClientSnapshot,
    current: &ClientSnapshot,
    alpha: f64,
) -> ClientSnapshot {
    let mut blended = current.clone();

    for (entity_id, entity) in &mut blended.game_state.entities {
        let Some(previous_entity) = previous.game_state.entities.get(entity_id) else {
            continue;
        };

        entity.x = lerp(previous_entity.x, entity.x, alpha);
        entity.y = lerp(previous_entity.y, entity.y, alpha);
        entity.vel_x = lerp(previous_entity.vel_x, entity.vel_x, alpha);
        entity.vel_y = lerp(previous_entity.vel_y, entity.vel_y, alpha);
    }

    for (controller_id, player) in &mut blended.game_state.players {
        let Some(previous_player) = previous.game_state.players.get(controller_id) else {
            continue;
        };

        let dir_x = lerp(previous_player.dir_x, player.dir_x, alpha);
        let dir_y = lerp(previous_player.dir_y, player.dir_y, alpha);
        let dir_len = (dir_x * dir_x + dir_y * dir_y).sqrt();
        if dir_len > 1e-6 {
            player.dir_x = dir_x / dir_len;
            player.dir_y = dir_y / dir_len;
        }
    }

    blended
}

fn lerp(start: f64, end: f64, alpha: f64) -> f64 {
    start + (end - start) * alpha
}