#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationRequest {
    pub pixel_data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub frame_number: u64,
}

impl PresentationRequest {
    pub fn new(pixel_data: Vec<u8>, width: u32, height: u32, frame_number: u64) -> Self {
        Self {
            pixel_data,
            width,
            height,
            frame_number,
        }
    }

    pub fn is_valid(&self) -> bool {
        let expected_len = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|px| px.checked_mul(4));

        match expected_len {
            Some(len) => self.pixel_data.len() == len,
            None => false,
        }
    }
}
