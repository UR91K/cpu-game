pub mod client;
pub mod server;
pub mod bot;

use crate::input::InputMessage;
use crate::model::PlayerId;
use crate::simulation::GameState;

pub trait Client: Send {
    fn id(&self) -> PlayerId;
    /// Return all inputs accumulated since the last call.
    fn poll_inputs(&mut self) -> Vec<InputMessage>;
    /// Called by the server after each authoritative tick.
    fn receive_state(&mut self, state: &GameState);
}
