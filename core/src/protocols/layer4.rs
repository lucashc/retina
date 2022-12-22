use crate::protocols::packet::ethernet::Ethernet;
use crate::protocols::packet::{ipv4::Ipv4, ipv6::Ipv6};
use crate::protocols::packet::tcp::{Tcp, TCP_PROTOCOL};
use crate::protocols::packet::udp::{Udp, UDP_PROTOCOL};
use crate::protocols::packet::Packet;
use crate::subscription::ZcFrame;

use anyhow::{bail, Result};

use tabled::{Style, Panel};
use tabled::builder::Builder;

use std::cmp;
use std::fmt;
use std::net::{IpAddr, SocketAddr};

/// Parsed transport-layer context from the packet used for connection tracking.
#[derive(Debug, Clone, Copy, Hash)]
pub struct L4Context {
    /// Source socket address.
    pub src: SocketAddr,
    /// Destination socket address.
    pub dst: SocketAddr,
    /// L4 protocol.
    pub proto: usize,
    /// Offset into the mbuf where payload begins.
    pub offset: usize,
    /// Length of the payload in bytes.
    pub length: usize,
    /// VLAN id
    pub vlan_id: Option<u16>
}

impl L4Context {
    pub fn new(mbuf: &ZcFrame) -> Result<Self> {
        if let Ok(eth) = mbuf.parse_to::<Ethernet>() {
            if let Ok(ipv4) = eth.parse_to::<Ipv4>() {
                if let Ok(tcp) = ipv4.parse_to::<Tcp>() {
                    if let Some(payload_size) = (ipv4.total_length() as usize)
                        .checked_sub(ipv4.header_len() + tcp.header_len())
                    {
                        Ok(L4Context {
                            src: SocketAddr::new(IpAddr::V4(ipv4.src_addr()), tcp.src_port()),
                            dst: SocketAddr::new(IpAddr::V4(ipv4.dst_addr()), tcp.dst_port()),
                            proto: TCP_PROTOCOL,
                            offset: tcp.next_header_offset(),
                            length: payload_size,
                            vlan_id: eth.get_last_vlan_id()
                        })
                    } else {
                        bail!("Malformed Packet");
                    }
                } else if let Ok(udp) = ipv4.parse_to::<Udp>() {
                    if let Some(payload_size) = (ipv4.total_length() as usize)
                        .checked_sub(ipv4.header_len() + udp.header_len())
                    {
                        Ok(L4Context {
                            src: SocketAddr::new(IpAddr::V4(ipv4.src_addr()), udp.src_port()),
                            dst: SocketAddr::new(IpAddr::V4(ipv4.dst_addr()), udp.dst_port()),
                            proto: UDP_PROTOCOL,
                            offset: udp.next_header_offset(),
                            length: payload_size,
                            vlan_id: eth.get_last_vlan_id()
                        })
                    } else {
                        bail!("Malformed Packet");
                    }
                } else {
                    bail!("Not TCP or UDP");
                }
            } else if let Ok(ipv6) = eth.parse_to::<Ipv6>() {
                if let Ok(tcp) = ipv6.parse_to::<Tcp>() {
                    if let Some(payload_size) =
                        (ipv6.payload_length() as usize).checked_sub(tcp.header_len())
                    {
                        Ok(L4Context {
                            src: SocketAddr::new(IpAddr::V6(ipv6.src_addr()), tcp.src_port()),
                            dst: SocketAddr::new(IpAddr::V6(ipv6.dst_addr()), tcp.dst_port()),
                            proto: TCP_PROTOCOL,
                            offset: tcp.next_header_offset(),
                            length: payload_size,
                            vlan_id: eth.get_last_vlan_id()
                        })
                    } else {
                        bail!("Malformed Packet");
                    }
                } else if let Ok(udp) = ipv6.parse_to::<Udp>() {
                    if let Some(payload_size) =
                        (ipv6.payload_length() as usize).checked_sub(udp.header_len())
                    {
                        Ok(L4Context {
                            src: SocketAddr::new(IpAddr::V6(ipv6.src_addr()), udp.src_port()),
                            dst: SocketAddr::new(IpAddr::V6(ipv6.dst_addr()), udp.dst_port()),
                            proto: UDP_PROTOCOL,
                            offset: udp.next_header_offset(),
                            length: payload_size,
                            vlan_id: eth.get_last_vlan_id()
                        })
                    } else {
                        bail!("Malformed Packet");
                    }
                } else {
                    bail!("Not TCP or UDP");
                }
            } else {
                bail!("Not IP");
            }
        } else {
            bail!("Not Ethernet");
        }
    }

    pub fn get_flow(&self) -> Flow {
        Flow(self.vlan_id, cmp::max(self.src, self.dst), cmp::min(self.src, self.dst), self.proto)
    }
}


#[derive(Debug, Clone, Copy, Hash)]
pub struct Flow(Option<u16>, SocketAddr, SocketAddr, usize);


impl fmt::Display for Flow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = Builder::default();
        builder.set_columns(["Vlan ID", "Address 1", "Address 2", "Protocol"]);
        let protocol = match self.3 {
            TCP_PROTOCOL => "TCP",
            UDP_PROTOCOL => "UDP",
            _ => "UNKOWN"
        };
        builder.add_record([format!("{:?}", self.0), self.1.to_string(), self.2.to_string(), protocol.into()]);
        let mut table = builder.build();
        table.with(Style::modern());
        table.with(Panel::header("Flow"));
        write!(f, "{table}")
    }
}