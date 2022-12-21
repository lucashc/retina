//! Zero-copy Ethernet frames.
//!
//! This is a packet-level subscription that delivers raw Ethernet frames in the order of arrival.
//! It has identical behavior to the [Frame](crate::subscription::frame::Frame) type, except is
//! zero-copy, meaning that callbacks are invoked on raw DPDK memory buffers instead of a
//! heap-allocated buffer. This is useful for performance sensitive applications that do not need to
//! store packet data. If ownership of the packet data is required, it is recommended to use
//! [Frame](crate::subscription::frame::Frame) instead.
//!
//! ## Warning
//! All `ZcFrame`s must be dropped (freed and returned to the memory pool) before the Retina runtime
//! is dropped.
//!
//! ## Example
//! Prints IPv4 packets with a TTL greater than 64:
//! ```
//! #[filter("ipv4.time_to_live > 64")]
//! fn main() {
//!     let config = default_config();
//!     let cb = |pkt: ZcFrame| {
//!         println!("{:?}", pkt.data());
//!         // implicit drop at end of scope
//!     };
//!     let mut runtime = Runtime::new(config, filter, cb).unwrap();
//!     runtime.run();
//!     // runtime dropped at end of scope
//! }
//! ```
use crate::memory::mbuf::Mbuf;
use crate::subscription::{Subscribable, Subscription};

use std::collections::HashMap;

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
///
/// It is recommended that `ZcFrame` be used for stateless packet analysis, and to use
/// [Frame](crate::subscription::Frame) instead if ownership of the packet is needed.
pub type ZcFrame = Mbuf;

impl Subscribable for ZcFrame {

    fn process_packet(
        mbuf: Mbuf,
        subscription: &Subscription<Self>,
    ) {
        subscription.invoke(mbuf);
    }
}