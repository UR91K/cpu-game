use std::sync::mpsc::Sender;

use crate::model::ControllerId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InputMessage {
    pub controller_id: ControllerId,
    #[allow(dead_code)]
    pub tick: u64,
    pub forward: bool,
    pub back: bool,
    pub strafe_left: bool,
    pub strafe_right: bool,
    pub fire: bool,
    /// Pre-scaled rotation angle in radians (already has mouse sensitivity applied)
    pub rotate_delta: f64,
}

pub trait InputSink {
    fn submit(&mut self, input: InputMessage);
}

pub struct ChannelInputSink {
    sender: Sender<InputMessage>,
}

impl ChannelInputSink {
    pub fn new(sender: Sender<InputMessage>) -> Self {
        Self { sender }
    }
}

impl InputSink for ChannelInputSink {
    fn submit(&mut self, input: InputMessage) {
        let _ = self.sender.send(input);
    }
}
