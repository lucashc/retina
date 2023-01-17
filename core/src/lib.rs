#![allow(clippy::needless_doctest_main)]
// #![warn(missing_docs)]

//! Retina-regex is a simple framework designed for high-speed deep packet inspection where only the payload of a packet is inspected.
//! It support parsing of any number of VLAN headers and both IPv4 and IPv6. It considers the payload as the payload after the UDP or TCP header.

#[macro_use]
mod timing;
pub mod config;
#[doc(hidden)]
#[allow(clippy::all)]
mod dpdk;
pub mod filter;
mod lcore;
mod memory;
pub mod packet_store;
mod port;
pub mod protocols;
pub mod rules;
mod runtime;
pub mod subscription;
pub mod utils;
pub use self::memory::mbuf::Mbuf;
pub use self::runtime::Runtime;

pub use dpdk::rte_rdtsc;
