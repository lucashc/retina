//! The filter module contains a special object `FilterCtx` that gets passed to every core and contains
//! * A shared hashmap of flows
//! * A timeout for the hashmap
//! * A thread-local copy of a `RegexSet`
//! * A sender to send packets non-blockingly for saving.

use dashmap::DashMap;

use crate::protocols::layer4::Flow;
use crate::subscription::ZcFrame;
use regex::bytes::RegexSet;
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Filter Context of which each core receives a local copy via a clone
#[derive(Debug)]
pub struct FilterCtx {
    /// Shared amongst all cores
    pub flows: Arc<DashMap<Flow, Instant>>,
    /// Shared amongst all cores
    pub timeout: Duration,
    /// Every core has local copy to prevent expensive locks
    /// We use an Arc here only to allow `Send` to the daemon thread
    /// The `Clone` trait implementation makes an explicit clone of the RegexSet.
    pub regexes: Arc<RwLock<RegexSet>>,
    /// Packet sender channel
    pub sender: Sender<(Flow, ZcFrame)>,
}

impl FilterCtx {
    /// Create a new FilterCtx
    pub fn new(
        reserve_capacity: usize,
        timeout: Duration,
        sender: Sender<(Flow, ZcFrame)>,
    ) -> FilterCtx {
        FilterCtx {
            flows: Arc::new(DashMap::with_capacity(reserve_capacity)),
            timeout,
            regexes: Arc::new(RwLock::new(RegexSet::empty())),
            sender,
        }
    }

    /// Check if the flow has been seen before
    /// This updates the timestamp of the flow automatically if it has been seen before
    pub fn check_if_existing_flow(&self, flow: &Flow) -> bool {
        // This function also updates the timeout when a match is made
        match self.flows.get_mut(flow) {
            Some(mut timestamp) => {
                *timestamp = Instant::now();
                true
            }
            None => false,
        }
    }

    // Add the flow to the hashmap with the current timestamp
    pub fn add_flow(&self, flow: &Flow) {
        self.flows.insert(flow.clone(), Instant::now());
    }

    /// Prune flows older than the timeout
    pub fn prune_flows(&self) {
        self.flows
            .retain(|_, timestamp| timestamp.elapsed() < self.timeout);
    }

    /// Check if the payload matches the regular expressions
    pub fn check_match(&self, payload: &[u8]) -> bool {
        self.regexes.read().unwrap().is_match(payload)
    }

    /// Sends a packet over the channel to be saved by receiver
    pub fn send_packet(&self, flow: &Flow, packet: ZcFrame) {
        self.sender.send((flow.clone(), packet)).unwrap();
    }
}

/// This is a custom `Clone` implementation to make sure that each thread receives its own regexset, so no clone of the `Arc`, but a new one.
impl Clone for FilterCtx {
    fn clone(&self) -> Self {
        Self {
            flows: self.flows.clone(),
            timeout: self.timeout.clone(),
            regexes: Arc::new(RwLock::new(self.regexes.read().unwrap().clone())),
            sender: self.sender.clone(),
        }
    }
}
