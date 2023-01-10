use dashmap::DashMap;

use crate::protocols::layer4::Flow;
use crate::subscription::ZcFrame;
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};
use std::time::{Instant, Duration};
use regex::bytes::RegexSet;

/// Filter Context of which each core receives a local copy via a clone
#[derive(Debug)]
pub struct FilterCtx {
    /// Shared amongst all cores
    flows: Arc<DashMap<Flow, Instant>>,
    /// Shared amongst all cores
    timeout: Duration,
    /// Every core has local copy to prevent expensive locks
    /// We use an Arc here only to allow `Send` to the daemon thread
    /// The `Clone` trait implementation makes an explicit clone of the RegexSet.
    pub(crate) regexes: Arc<RwLock<RegexSet>>,
    /// Packet sender channel
    sender: Sender<(Flow, ZcFrame)>
}

impl FilterCtx {
    pub fn new(reserve_capacity: usize, timeout: Duration, sender: Sender<(Flow, ZcFrame)>) -> FilterCtx {
        FilterCtx {
            flows: Arc::new(DashMap::with_capacity(reserve_capacity)),
            timeout,
            regexes: Arc::new(RwLock::new(RegexSet::empty())),
            sender
        }
    }

    pub fn check_if_existing_flow(&self, flow: &Flow) -> bool {
        // This function also updates the timeout when a match is made
        match self.flows.get_mut(flow) {
            Some(mut timestamp) => {
                *timestamp = Instant::now();
                true
            },
            None => false
        }
    }

    pub fn add_flow(&self, flow: &Flow) {
        self.flows.insert(flow.clone(), Instant::now());
    }

    pub fn prune_flows(&self) {
        self.flows.retain(|_, timestamp| timestamp.elapsed() < self.timeout);
    }

    pub fn check_match(&self, payload: &[u8]) -> bool{
        self.regexes.read().unwrap().is_match(payload)
    }

    pub fn send_packet(&self, flow: &Flow, packet: ZcFrame) {
        self.sender.send((flow.clone(), packet)).unwrap();
    }
    
}

impl Clone for FilterCtx {
    fn clone(&self) -> Self {
        Self { 
            flows: self.flows.clone(), 
            timeout: self.timeout.clone(), 
            regexes: Arc::new(RwLock::new(self.regexes.read().unwrap().clone())),
            sender: self.sender.clone()
        }
    }
}