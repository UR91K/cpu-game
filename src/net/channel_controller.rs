use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use super::Controller;
use crate::input::InputMessage;
use crate::model::ControllerId;
use crate::runtime::{AuthoritativeUpdate, TransportDebug};
use crate::simulation::GameState;

#[derive(Default)]
pub struct ChannelTransportState {
    pub received_count: u64,
    pub polled_count: u64,
    pub last_received_input: Option<InputMessage>,
    pub last_polled_input: Option<InputMessage>,
}

pub struct ChannelController {
    id: ControllerId,
    input_rx: Receiver<InputMessage>,
    update_tx: Sender<AuthoritativeUpdate>,
    transport_state: Arc<Mutex<ChannelTransportState>>,
}

impl ChannelController {
    pub fn new(
        id: ControllerId,
        input_rx: Receiver<InputMessage>,
        update_tx: Sender<AuthoritativeUpdate>,
        transport_state: Arc<Mutex<ChannelTransportState>>,
    ) -> Self {
        Self {
            id,
            input_rx,
            update_tx,
            transport_state,
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
                Ok(mut input) => {
                    input.controller_id = self.id;
                    let mut transport_state = self.transport_state.lock().unwrap();
                    transport_state.polled_count += 1;
                    transport_state.last_polled_input = Some(input.clone());
                    inputs.push(input);
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }

        inputs
    }

    fn receive_state(&mut self, state: &GameState) {
        let transport_debug = {
            let transport_state = self.transport_state.lock().unwrap();
            TransportDebug {
                received_count: transport_state.received_count,
                polled_count: transport_state.polled_count,
                last_received_input: transport_state.last_received_input.clone(),
                last_polled_input: transport_state.last_polled_input.clone(),
            }
        };
        let _ = self
            .update_tx
            .send(AuthoritativeUpdate::from_game_state(
                state.clone(),
                Some(self.id),
                Some(transport_debug),
            ));
    }
}