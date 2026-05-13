use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use super::Controller;
use crate::input::InputMessage;
use crate::model::ControllerId;
use crate::runtime::AuthoritativeUpdate;
use crate::simulation::GameState;

pub struct ChannelController {
    id: ControllerId,
    input_rx: Receiver<InputMessage>,
    update_tx: Sender<AuthoritativeUpdate>,
}

impl ChannelController {
    pub fn new(
        id: ControllerId,
        input_rx: Receiver<InputMessage>,
        update_tx: Sender<AuthoritativeUpdate>,
    ) -> Self {
        Self {
            id,
            input_rx,
            update_tx,
        }
    }
}

impl Controller for ChannelController {
    fn id(&self) -> ControllerId {
        self.id
    }

    fn poll_inputs(&mut self) -> Vec<InputMessage> {
        let mut inputs = Vec::new();

        loop {
            match self.input_rx.try_recv() {
                Ok(input) => inputs.push(input),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }

        inputs
    }

    fn receive_state(&mut self, state: &GameState) {
        let _ = self
            .update_tx
            .send(AuthoritativeUpdate::from_game_state(state.clone()));
    }
}