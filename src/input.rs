use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

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

pub type SharedInputHistory = Arc<Mutex<Vec<InputMessage>>>;

pub trait InputSink {
    fn submit(&mut self, input: InputMessage);
}

pub struct ChannelInputSink {
    sender: Sender<InputMessage>,
    pending_inputs: SharedInputHistory,
}

impl ChannelInputSink {
    pub fn new(sender: Sender<InputMessage>, pending_inputs: SharedInputHistory) -> Self {
        Self {
            sender,
            pending_inputs,
        }
    }
}

impl InputSink for ChannelInputSink {
    fn submit(&mut self, input: InputMessage) {
        if self.sender.send(input.clone()).is_ok() {
            self.pending_inputs.lock().unwrap().push(input);
        }
    }
}
