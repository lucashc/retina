//! Ethernet packet.

use crate::memory::mbuf::Mbuf;
use crate::protocols::packet::{Packet, PacketHeader, PacketParseError};
use crate::utils::types::*;

use anyhow::{bail, Result, anyhow};
use pnet::datalink::MacAddr;

// Ethernet Header size
const HDR_SIZE: usize = 14;

// VLAN tag size and type
const TAG_SIZE: usize = 4;
const VLAN_802_1Q: usize = 0x8100;

/// An Ethernet frame.
///
/// On networks that support virtual LANs, the frame may include a VLAN tag after the source MAC
/// address. Double-tagged frames (QinQ) are not yet supported.
#[derive(Debug)]
pub struct Ethernet<'a> {
    /// Fixed header.
    header: EthernetHeader,
    /// Possible VLAN headers
    vlan_headers: Vec<VlanHeader>,
    /// Offset to `header` from the start of `mbuf`.
    offset: usize,
    /// Packet buffer.
    mbuf: &'a Mbuf,
}

impl<'a> Ethernet<'a> {
    /// Returns the destination MAC address.
    #[inline]
    pub fn dst(&self) -> MacAddr {
        self.header.dst
    }

    /// Returns the source MAC address.
    #[inline]
    pub fn src(&self) -> MacAddr {
        self.header.src
    }

    /// Returns the encapsulated protocol identifier for untagged and single-tagged frames, and `0`
    /// for incorrectly fornatted and (not yet supported) double-tagged frames,.
    #[inline]
    pub fn ether_type(&self) -> u16 {
        self.next_header().unwrap_or(0) as u16
    }
}

impl<'a> Packet<'a> for Ethernet<'a> {
    fn mbuf(&self) -> &Mbuf {
        self.mbuf
    }

    fn header_len(&self) -> usize {
        self.header.length() + self.vlan_headers.iter().fold(0, |sum, val: &VlanHeader| sum + val.length())
    }

    fn next_header_offset(&self) -> usize {
        self.offset + self.header_len()
    }

    fn next_header(&self) -> Option<usize> {
        let ether_type = if self.vlan_headers.is_empty() {
            u16::from(self.header.ether_type)
        } else {
            u16::from(self.vlan_headers.last().unwrap().ether_type)
        };
        Some(ether_type.into())
    }

    fn parse_from(outer: &'a impl Packet<'a>) -> Result<Self>
    where
        Self: Sized,
    {
        if let Ok(header) = outer.mbuf().get_data(0) {
            let current_header: EthernetHeader = unsafe { *header };
            let vlan_headers = if u16::from(current_header.ether_type) as usize == VLAN_802_1Q {
                let mut vlans = vec![];
                let mut offset = current_header.length();
                loop {
                    let next: *const VlanHeader = outer.mbuf().get_data(offset).map_err(|_| anyhow!(PacketParseError::InvalidRead))?;
                    vlans.push(unsafe { *next });
                    if u16::from(vlans.last().unwrap().ether_type) as usize == VLAN_802_1Q {
                        offset += vlans.last().unwrap().length();
                    } else {
                        break vlans;
                    }
                }
            } else {
                vec![]
            };
            Ok(Ethernet {
                header: unsafe { *header },
                vlan_headers,
                offset: 0,
                mbuf: outer.mbuf(),
            })
        } else {
            bail!(PacketParseError::InvalidRead)
        }
    }
}

/// Fixed portion of an Ethernet header.
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct EthernetHeader {
    dst: MacAddr,
    src: MacAddr,
    ether_type: u16be,
}

impl PacketHeader for EthernetHeader {
    fn length(&self) -> usize {
        HDR_SIZE
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct VlanHeader {
    tci: u16be,
    ether_type: u16be
}

impl PacketHeader for VlanHeader {
    fn length(&self) -> usize {
        TAG_SIZE
    }
}