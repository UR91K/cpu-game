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
