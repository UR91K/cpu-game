use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use crate::clock::ClockManager;
use crate::input::{InputMessage, SharedInputHistory};
use crate::model::{ControllerId, EntityId, Level};
use crate::simulation::{GameState, TICK_DT, TICK_RATE, tick};
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
pub struct PredictionDebug {
    pub acked_input_tick: u64,
    pub pending_input_count: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientSnapshot {
    pub game_state: GameState,
    pub authoritative_tick: u64,
    pub local_controller_id: Option<ControllerId>,
    pub transport_debug: Option<TransportDebug>,
    pub prediction_debug: Option<PredictionDebug>,
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
            transport_debug,
            prediction_debug: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthoritativeUpdate {
    pub ack: u64,
    pub snapshot: ClientSnapshot,
}

impl AuthoritativeUpdate {
    pub fn from_game_state(
        game_state: GameState,
        local_controller_id: Option<ControllerId>,
        ack: u64,
        transport_debug: Option<TransportDebug>,
    ) -> Self {
        Self {
            ack,
            snapshot: ClientSnapshot::from_game_state(
                game_state,
                local_controller_id,
                transport_debug,
            ),
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
    last_acked_input_tick: u64,
    pending_inputs: SharedInputHistory,
}

#[allow(dead_code)]
impl ChannelClientRuntime {
    pub fn new(
        level: Arc<Level>,
        updates: Receiver<AuthoritativeUpdate>,
        pending_inputs: SharedInputHistory,
    ) -> Self {
        Self {
            updates,
            client: SnapshotRuntime::new(level),
            previous_snapshot: None,
            last_update_at: None,
            last_acked_input_tick: 0,
            pending_inputs,
        }
    }

    fn drain_updates(&mut self) {
        while let Ok(update) = self.updates.try_recv() {
            let acked_input_tick = update.ack;
            self.pending_inputs
                .lock()
                .unwrap()
                .retain(|input| input.tick > acked_input_tick);
            self.previous_snapshot = self.client.snapshot();
            self.last_acked_input_tick = acked_input_tick;
            self.client.apply_update(update);
            self.last_update_at = Some(Instant::now());
        }
    }

    fn predicted_snapshot(&self) -> Option<ClientSnapshot> {
        let authoritative = self.client.snapshot()?;
        let local_controller_id = authoritative.local_controller_id?;
        let acked_input_tick = self.last_acked_input_tick;
        // Cap replayed inputs to one second of ticks. Without this, a stale pending queue
        // (e.g. from inputs accumulating while the server process is starting) can replay
        // an entire jump arc in a single prediction pass, making the jump appear instant.
        const MAX_PENDING: usize = TICK_RATE as usize;
        let pending_inputs = self
            .pending_inputs
            .lock()
            .unwrap()
            .iter()
            .filter(|input| {
                input.controller_id == local_controller_id && input.tick > acked_input_tick
            })
            .cloned()
            .collect::<Vec<_>>();

        // Take only the most recent MAX_PENDING inputs so old stale entries don't replay
        // completed physics arcs.
        let pending_inputs = if pending_inputs.len() > MAX_PENDING {
            pending_inputs[pending_inputs.len() - MAX_PENDING..].to_vec()
        } else {
            pending_inputs
        };

        if pending_inputs.is_empty() {
            return None;
        }

        let mut predicted = authoritative.clone();
        predicted.game_state = replay_predicted_inputs(
            &authoritative.game_state,
            &pending_inputs,
            self.client.level(),
        );
        predicted.authoritative_tick = authoritative.authoritative_tick;
        predicted.prediction_debug = Some(PredictionDebug {
            acked_input_tick,
            pending_input_count: pending_inputs.len(),
        });
        Some(predicted)
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
        self.predicted_snapshot()
            .or_else(|| self.interpolated_snapshot())
            .or_else(|| self.client.snapshot())
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

fn replay_predicted_inputs(state: &GameState, pending_inputs: &[InputMessage], level: &Level) -> GameState {
    let mut predicted = state.clone();
    for input in pending_inputs {
        predicted = tick(&predicted, std::slice::from_ref(input), level, TICK_DT);
    }
    predicted
}

fn interpolate_snapshot(
    previous: &ClientSnapshot,
    current: &ClientSnapshot,
    alpha: f64,
) -> ClientSnapshot {
    let mut blended = current.clone();
    blended.prediction_debug = None;

    for (entity_id, entity) in &mut blended.game_state.entities {
        let Some(previous_entity) = previous.game_state.entities.get(entity_id) else {
            continue;
        };

        entity.x = lerp(previous_entity.x, entity.x, alpha);
        entity.y = lerp(previous_entity.y, entity.y, alpha);
        entity.z = lerp(previous_entity.z, entity.z, alpha);
        entity.vel_x = lerp(previous_entity.vel_x, entity.vel_x, alpha);
        entity.vel_y = lerp(previous_entity.vel_y, entity.vel_y, alpha);
        entity.vel_z = lerp(previous_entity.vel_z, entity.vel_z, alpha);
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

#[cfg(test)]
mod tests {
    use crate::input::InputMessage;
    use crate::model::Level;
    use crate::simulation::{GameState, Player};

    use super::replay_predicted_inputs;

    #[test]
    fn replay_predicted_inputs_advances_one_tick_per_pending_input() {
        let level = Level::new(vec![vec![0, 0], vec![0, 0]]);
        let mut state = GameState::new();
        let pawn_id = state.spawn_pawn(0.5, 0.5, Some(1));
        state.players.insert(1, Player::new(pawn_id));

        let pending_inputs = vec![
            InputMessage {
                controller_id: 1,
                jump: true,
                ..InputMessage::default()
            },
            InputMessage {
                controller_id: 1,
                forward: true,
                ..InputMessage::default()
            },
        ];

        let predicted = replay_predicted_inputs(&state, &pending_inputs, &level);

        assert_eq!(predicted.tick, state.tick + pending_inputs.len() as u64);
    }
}
