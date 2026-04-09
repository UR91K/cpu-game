use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompositeParams {
    pub brightness: f32,
    pub saturation: f32,
    pub artifacting: f32,
    pub fringing: f32,
    pub ntsc_field_rate: f64,
    pub horizontal_scale: u32,
    pub source_gamma: f32,
    pub target_gamma: f32,
    pub noise_amplitude: f32,
}

impl Default for CompositeParams {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            saturation: 1.0,
            artifacting: 1.0,
            fringing: 1.0,
            ntsc_field_rate: 29.97,
            horizontal_scale: 4,
            source_gamma: 2.5,
            target_gamma: 2.0,
            noise_amplitude: 0.0,
        }
    }
}

impl CompositeParams {
    pub fn mix_mat(&self) -> [[f32; 3]; 3] {
        [
            [self.brightness, self.fringing, self.fringing],
            [self.artifacting, 2.0 * self.saturation, 0.0],
            [self.artifacting, 0.0, 2.0 * self.saturation],
        ]
    }

    pub fn gamma_exp(&self) -> f32 {
        self.source_gamma / self.target_gamma
    }
}
