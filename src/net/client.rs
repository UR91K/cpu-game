use std::collections::VecDeque;

use crate::{model::{InputMessage, Map, PlayerId, TICK_DT}, simulation::{GameState, tick}};

pub struct LocalClient {
    pub id: PlayerId,
    pub predicted_state: GameState,
    pub pending_inputs: VecDeque<InputMessage>,
    pub last_acknowledged_tick: u64,
}

impl LocalClient {
    pub fn reconcile(&mut self, server_state: &GameState, map: &Map) {
        self.predicted_state = server_state.clone();

        for input in self.pending_inputs.iter().filter(|input| input.tick > self.last_acknowledged_tick) {
            self.predicted_state = tick(&self.predicted_state, &[input.clone()], map);
        }
    }
}