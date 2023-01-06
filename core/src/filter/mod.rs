use dashmap::DashMap;

use crate::protocols::layer4::Flow;
use std::sync::{Arc, RwLock};
use std::time::{Instant, Duration};
use regex::bytes::RegexSet;


#[derive(Debug)]
pub struct FilterCtx {
    flows: Arc<DashMap<Flow, Instant>>,
    timeout: Arc<Duration>,
    regexes: RwLock<RegexSet>
}

impl FilterCtx {
    pub fn new(reserve_capacity: usize, timeout: Duration, regexes: RegexSet) -> FilterCtx {
        FilterCtx {
            flows: Arc::new(DashMap::with_capacity(reserve_capacity)),
            timeout: Arc::new(timeout),
            regexes: RwLock::new(regexes)
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
        self.flows.retain(|_, timestamp| timestamp.elapsed() < *self.timeout);
    }

    pub fn check_match(&self, payload: &[u8]) -> bool{
        self.regexes.read().unwrap().is_match(payload)
    }
    
}

impl Clone for FilterCtx {
    fn clone(&self) -> Self {
        Self { 
            flows: self.flows.clone(), 
            timeout: self.timeout.clone(), 
            regexes: RwLock::new(self.regexes.read().unwrap().clone())
        }
    }
}