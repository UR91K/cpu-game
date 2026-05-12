use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::model::ControllerId;

#[derive(Clone, Debug, Default)]
pub struct InputMessage {
    pub controller_id: ControllerId,
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

pub struct LocalInputSink {
    queue: Arc<Mutex<VecDeque<InputMessage>>>,
}

impl LocalInputSink {
    pub fn new(queue: Arc<Mutex<VecDeque<InputMessage>>>) -> Self {
        Self { queue }
    }
}

impl InputSink for LocalInputSink {
    fn submit(&mut self, input: InputMessage) {
        self.queue.lock().unwrap().push_back(input);
    }
}
