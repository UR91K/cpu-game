use crate::{model::{InputMessage, PlayerId}, simulation::GameState};
mod client;

pub trait Client: Send {
    fn id(&self) -> PlayerId;
    fn send(&mut self) -> Vec<InputMessage>;
    fn recieve(&mut self, state: &GameState);
}