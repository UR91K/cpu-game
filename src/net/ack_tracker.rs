pub struct AckTracker {
    last_received_sequence: u16,
    received_bits: u32,
}

impl Default for AckTracker {
    fn default() -> Self {
        Self {
            last_received_sequence: 0,
            received_bits: 0,
        }
    }
}

impl AckTracker {
    pub fn record(&mut self, sequence: u16) {
        let diff = sequence.wrapping_sub(self.last_received_sequence);
        if diff == 0 {
            return;
        }

        if diff < 32768 {
            let shift = u32::from(diff.min(32));
            self.received_bits = if shift >= 32 {
                0
            } else {
                self.received_bits << shift
            };
            if diff <= 32 {
                self.received_bits |= 1u32 << (diff - 1);
            }
            self.last_received_sequence = sequence;
            return;
        }

        let reverse_diff = self.last_received_sequence.wrapping_sub(sequence);
        if (1..=32).contains(&reverse_diff) {
            self.received_bits |= 1u32 << (reverse_diff - 1);
        }
    }

    pub fn ack(&self) -> u16 {
        self.last_received_sequence
    }

    pub fn ack_bits(&self) -> u32 {
        self.received_bits
    }

    #[allow(dead_code)]
    pub fn is_acked(&self, sequence: u16) -> bool {
        let diff = self.last_received_sequence.wrapping_sub(sequence);
        if diff == 0 {
            return true;
        }
        if diff >= 33 {
            return false;
        }
        self.received_bits & (1u32 << (diff - 1)) != 0
    }

    pub fn acked_by_remote(ack: u16, ack_bits: u32, sequence: u16) -> bool {
        let diff = ack.wrapping_sub(sequence);
        if diff == 0 {
            return true;
        }
        if diff >= 33 {
            return false;
        }
        ack_bits & (1u32 << (diff - 1)) != 0
    }
}
