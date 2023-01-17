//! Zero-copy Ethernet frames.
//!
//! This is a packet-level subscription that delivers raw Ethernet frames in the order of arrival.
//! It is zero-copy, meaning that callbacks are invoked on raw DPDK memory buffers instead of a
//! heap-allocated buffer. This is useful for performance sensitive applications that do not need to
//! store packet data.
//!
//! ## Warning
//! All `ZcFrame`s must be dropped (freed and returned to the memory pool) before the Retina runtime
//! is dropped.
use crate::filter::FilterCtx;
use crate::memory::mbuf::Mbuf;
use crate::subscription::{Subscribable, Subscription};

/// A zero-copy Ethernet frame.
///
/// ## Remarks
/// This is a type alias of a DPDK message buffer. Retina allows subscriptions on raw DPDK memory
/// buffers with zero-copy (i.e., without copying into a heap-allocated buffer). This is useful for
/// performance sensitive applications that do not need to store packet data.
///
/// However, the callback does not obtain ownership of the packet. Therefore, all `ZcFrame`s must be
/// dropped before the runtime is dropped, or a segmentation fault may occur when the memory pools
/// are de-allocated. Storing `ZcFrame`s also reduces the number of available packet buffers for
/// incoming packets and can cause memory pool exhaustion.
pub type ZcFrame = Mbuf;

impl Subscribable for ZcFrame {
    fn process_packet(mbuf: Mbuf, filter_ctx: &FilterCtx, subscription: &Subscription<Self>) {
        subscription.invoke(mbuf, filter_ctx);
    }
}
