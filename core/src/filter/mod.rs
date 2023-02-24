//! The filter module contains a special object `FilterCtx` that gets passed to every core and contains
//! * A shared hashmap of flows
//! * A timeout for the hashmap
//! * A thread-local copy of a `RegexSet`
//! * A sender to send packets non-blockingly for saving.



use crate::protocols::layer4::Flow;
use crate::subscription::ZcFrame;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::mpsc::Sender;



/// Filter Context of which each core receives a local copy via a clone
#[derive(Debug)]
pub struct FilterCtx {
    /// Packet sender channel
    pub senders: Vec<Sender<(Flow, ZcFrame)>>,
}

impl FilterCtx {
    /// Create a new FilterCtx
    pub fn new(
        senders: Vec<Sender<(Flow, ZcFrame)>>,
    ) -> FilterCtx {
        FilterCtx { senders }
    }

    /// Sends a packet over the channel to be saved by receiver
    pub fn send_packet(&self, flow: &Flow, packet: ZcFrame) {
        let mut hasher = DefaultHasher::new();
        flow.hash(&mut hasher);
        let hash = hasher.finish();
        self.senders[(hash as usize) % self.senders.len()].send((flow.clone(), packet)).unwrap();
    }
}

/// This is a custom `Clone` implementation to make sure that each thread receives its own regexset, so no clone of the `Arc`, but a new one.
impl Clone for FilterCtx {
    fn clone(&self) -> Self {
        Self { senders: self.senders.clone() }
    }
}
