use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use super::Controller;
use crate::input::InputMessage;
use crate::model::{ControllerId, Level};
use crate::simulation::{GameState, TICK_DT, tick};

/// The human player's client-side handle.
///
/// `input_queue` is shared with `App` — the app pushes `InputMessage`s into it
/// while this struct drains them when the server calls `poll_inputs()`.
pub struct LocalController {
    pub id: ControllerId,
    /// Re-simulated state for rendering (keeps inputs the server hasn't confirmed yet).
    pub predicted_state: GameState,
    pub input_queue: Arc<Mutex<VecDeque<InputMessage>>>,
    pub last_acked_tick: u64,
    level: Arc<Level>,
}

impl LocalController {
    pub fn new(
        id: ControllerId,
        initial_state: GameState,
        input_queue: Arc<Mutex<VecDeque<InputMessage>>>,
        level: Arc<Level>,
    ) -> Self {
        Self {
            id,
            predicted_state: initial_state,
            input_queue,
            last_acked_tick: 0,
            level,
        }
    }

    /// Accept the server's authoritative state and replay any unacknowledged inputs on top.
    pub fn reconcile(&mut self, server_state: &GameState) {
        self.predicted_state = server_state.clone();
        let queue = self.input_queue.lock().unwrap();
        for input in queue.iter().filter(|i| i.tick > self.last_acked_tick) {
            self.predicted_state = tick(
                &self.predicted_state,
                &[input.clone()],
                &self.level,
                TICK_DT,
            );
        }
    }
}

impl Controller for LocalController {
    fn id(&self) -> ControllerId {
        self.id
    }

    fn poll_inputs(&mut self) -> Vec<InputMessage> {
        let mut queue = self.input_queue.lock().unwrap();
        queue.drain(..).collect()
    }

    fn receive_state(&mut self, state: &GameState) {
        self.last_acked_tick = state.tick;
        self.reconcile(state);
    }
}
