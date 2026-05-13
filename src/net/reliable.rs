use std::collections::VecDeque;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::model::{ControllerId, EntityId, PickupKind};
use crate::runtime::SoundEvent;

use super::ack_tracker::AckTracker;

const RETRANSMIT_MS: u64 = 100;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReliablePayload {
    pub sequence: u16,
    pub message: ReliableMessage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ReliableMessage {
    PickupCollected {
        entity_id: EntityId,
        kind: PickupKind,
    },
    PlayerSpawned {
        controller_id: ControllerId,
        entity_id: EntityId,
    },
    PlayerDied {
        controller_id: ControllerId,
    },
    SoundEvent(SoundEvent),
}

struct PendingReliable {
    sequence: u16,
    payload: ReliableMessage,
    last_sent_at: Option<Instant>,
    last_sent_packet: Option<u16>,
}

#[derive(Default)]
pub struct ReliableChannel {
    pending: VecDeque<PendingReliable>,
    #[allow(dead_code)]
    next_sequence: u16,
}

impl ReliableChannel {
    #[allow(dead_code)]
    pub fn enqueue(&mut self, message: ReliableMessage) {
        self.pending.push_back(PendingReliable {
            sequence: self.next_sequence,
            payload: message,
            last_sent_at: None,
            last_sent_packet: None,
        });
        self.next_sequence = self.next_sequence.wrapping_add(1);
    }

    pub fn on_ack(&mut self, ack: u16, ack_bits: u32) {
        self.pending.retain(|msg| {
            let Some(packet_sequence) = msg.last_sent_packet else {
                return true;
            };
            !AckTracker::acked_by_remote(ack, ack_bits, packet_sequence)
        });
    }

    pub fn collect_for_send(&mut self, packet_sequence: u16) -> Vec<ReliablePayload> {
        let now = Instant::now();
        self.pending
            .iter_mut()
            .filter(|msg| {
                msg.last_sent_at.is_none_or(|last_sent_at| {
                    now.duration_since(last_sent_at).as_millis() as u64 >= RETRANSMIT_MS
                })
            })
            .map(|msg| {
                msg.last_sent_at = Some(now);
                msg.last_sent_packet = Some(packet_sequence);
                ReliablePayload {
                    sequence: msg.sequence,
                    message: msg.payload.clone(),
                }
            })
            .collect()
    }
}
