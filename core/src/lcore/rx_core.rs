use super::CoreId;
use crate::dpdk;
use crate::filter::FilterCtx;
use crate::memory::mbuf::Mbuf;
use crate::port::{RxQueue, RxQueueType};
use crate::subscription::*;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use itertools::Itertools;

/// A RxCore polls from `rxqueues` and reduces the stream of packets into
/// a stream of higher-level network events to be processed by the user.
pub(crate) struct RxCore<'a, S>
where
    S: Subscribable,
{
    pub(crate) id: CoreId,
    pub(crate) rxqueues: Vec<RxQueue>,
    pub(crate) subscription: Arc<Subscription<'a, S>>,
    pub(crate) filter_ctx: FilterCtx,
    pub(crate) is_running: Arc<AtomicBool>,
}

impl<'a, S> RxCore<'a, S>
where
    S: Subscribable,
{
    /// This creates a new `RXCore`. Note that a `FilterCtx` object is passed along. This object gets cloned such that each `RxCore` has its own local copy.
    pub(crate) fn new(
        core_id: CoreId,
        rxqueues: Vec<RxQueue>,
        subscription: Arc<Subscription<'a, S>>,
        filter_ctx: &FilterCtx,
        is_running: Arc<AtomicBool>,
    ) -> Self {
        RxCore {
            id: core_id,
            rxqueues,
            subscription,
            filter_ctx: filter_ctx.clone(),
            is_running,
        }
    }

    pub(crate) fn rx_burst(&self, rxqueue: &RxQueue, rx_burst_size: u16) -> Vec<Mbuf> {
        let mut ptrs = Vec::with_capacity(rx_burst_size as usize);
        let nb_rx = unsafe {
            dpdk::rte_eth_rx_burst(
                rxqueue.pid.raw(),
                rxqueue.qid.raw(),
                ptrs.as_mut_ptr(),
                rx_burst_size,
            )
        };
        unsafe {
            ptrs.set_len(nb_rx as usize);
            ptrs.into_iter()
                .map(Mbuf::new_unchecked)
                .collect::<Vec<Mbuf>>()
        }
    }

    pub(crate) fn rx_loop(&self) {
        // TODO: need check to enforce that each core only has same queue types
        if self.rxqueues[0].ty == RxQueueType::Receive {
            self.rx_process();
        } else {
            self.rx_sink();
        }
    }

    fn rx_process(&self) {
        log::info!(
            "Launched RX on core {}, polling {}",
            self.id,
            self.rxqueues.iter().format(", "),
        );

        let mut nb_pkts = 0;
        let mut nb_bytes = 0;

        while self.is_running.load(Ordering::Relaxed) {
            for rxqueue in self.rxqueues.iter() {
                let mbufs: Vec<Mbuf> = self.rx_burst(rxqueue, 32);
                for mbuf in mbufs.into_iter() {
                    log::debug!("{:#?}", mbuf);
                    log::debug!("Mark: {}", mbuf.mark());
                    log::debug!("RSS Hash: 0x{:x}", mbuf.rss_hash());
                    log::debug!(
                        "Queue ID: {}, Port ID: {}, Core ID: {}",
                        rxqueue.qid,
                        rxqueue.pid,
                        self.id,
                    );
                    nb_pkts += 1;
                    nb_bytes += mbuf.data_len() as u64;
                    S::process_packet(mbuf, &self.filter_ctx, &self.subscription);
                }
            }
        }

        log::info!(
            "Core {} total recv from {}: {} pkts, {} bytes",
            self.id,
            self.rxqueues.iter().format(", "),
            nb_pkts,
            nb_bytes
        );
    }

    fn rx_sink(&self) {
        log::info!(
            "Launched SINK on core {}, polling {}",
            self.id,
            self.rxqueues.iter().format(", "),
        );

        let mut nb_pkts = 0;
        let mut nb_bytes = 0;

        while self.is_running.load(Ordering::Relaxed) {
            for rxqueue in self.rxqueues.iter() {
                let mbufs: Vec<Mbuf> = self.rx_burst(rxqueue, 32);
                for mbuf in mbufs.into_iter() {
                    log::debug!("RSS Hash: 0x{:x}", mbuf.rss_hash());
                    log::debug!(
                        "Queue ID: {}, Port ID: {}, Core ID: {}",
                        rxqueue.qid,
                        rxqueue.pid,
                        self.id,
                    );
                    nb_pkts += 1;
                    nb_bytes += mbuf.data_len() as u64;
                }
            }
        }
        log::info!(
            "Sink Core {} total recv from {}: {} pkts, {} bytes",
            self.id,
            self.rxqueues.iter().format(", "),
            nb_pkts,
            nb_bytes
        );
    }
}
