pub mod bots;
pub mod channel_controller;
pub mod server;
pub mod tcp;

use crate::input::InputMessage;
use crate::model::ControllerId;
use crate::simulation::GameState;

pub trait Controller: Send {
    fn id(&self) -> ControllerId;
    /// Return all inputs accumulated since the last call.
    fn poll_inputs(&mut self) -> Vec<InputMessage>;
    /// Called by the server after each authoritative tick.
    fn receive_state(&mut self, state: &GameState);
}
