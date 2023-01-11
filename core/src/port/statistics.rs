use super::PortId;
use crate::dpdk;

use indexmap::IndexMap;
use std::ffi::CStr;
use std::mem;

use anyhow::{bail, Result};
use tabled::{
    builder::Builder, object::FirstRow, row, Concat, Disable, Panel, Style, Table, TableIteratorExt,
};

/// Collects extended statistics
#[derive(Debug)]
pub(crate) struct PortStats {
    pub(crate) stats: IndexMap<String, u64>,
    pub(crate) port_id: PortId,
}

impl PortStats {
    /// Retrieve port statistics at current time
    pub(crate) fn collect(port_id: PortId) -> Result<Self> {
        // temporary table used to get number of available statistics
        let mut table: Vec<dpdk::rte_eth_xstat> = vec![];
        let len = unsafe { dpdk::rte_eth_xstats_get(port_id.raw(), table.as_mut_ptr(), 0) };
        if len < 0 {
            bail!("Invalid Port ID: {}", port_id);
        }

        let mut labels = Vec::with_capacity(len as usize);
        for _ in 0..len {
            let xstat_name: dpdk::rte_eth_xstat_name = unsafe { mem::zeroed() };
            labels.push(xstat_name);
        }

        let nb_labels = unsafe {
            dpdk::rte_eth_xstats_get_names(port_id.raw(), labels.as_mut_ptr(), len as u32)
        };
        if nb_labels < 0 || nb_labels > len {
            bail!("Failed to retrieve port statistics labels.");
        }

        let mut xstats = Vec::with_capacity(len as usize);
        for _ in 0..len {
            let xstat: dpdk::rte_eth_xstat = unsafe { mem::zeroed() };
            xstats.push(xstat);
        }
        let nb_xstats =
            unsafe { dpdk::rte_eth_xstats_get(port_id.raw(), xstats.as_mut_ptr(), len as u32) };
        if nb_xstats < 0 || nb_xstats > len {
            bail!("Failed to retrieve port statistics.");
        }

        if nb_labels != nb_xstats {
            bail!("Number of labels does not match number of retrieved statistics.");
        }

        let mut stats = IndexMap::new();
        for i in 0..nb_xstats {
            let label = unsafe { CStr::from_ptr(labels[i as usize].name.as_ptr()) };
            let value = xstats[i as usize].value;
            stats.insert(label.to_string_lossy().into_owned(), value);
        }
        Ok(PortStats { stats, port_id })
    }

    /// Displays all statistics with keyword in list of keywords
    pub(crate) fn display(&self, keywords: &[String]) {
        // println!("Port {} statistics", self.port_id);
        let mut capture = self.display_capture_rate();
        let mut out_of_buffer = self.display_out_of_buffer_rate();
        let mut discard_rate = self.display_discard_rate();

        capture.with(Disable::row(FirstRow));
        out_of_buffer.with(Disable::row(FirstRow));
        discard_rate.with(Disable::row(FirstRow));

        capture.with(Concat::vertical(out_of_buffer));
        capture.with(Concat::vertical(discard_rate));
        capture.with(Style::modern());

        let mut builder_keywords = Builder::default();
        for (label, value) in self.stats.iter() {
            if keywords.iter().any(|k| label.contains(k)) {
                builder_keywords.add_record([label.clone(), value.to_string()]);
            }
        }
        let mut table_keywords = builder_keywords.build();
        table_keywords.with(Style::modern());

        let mut complete = row![capture, table_keywords];
        complete.with(Panel::header(format!("Port {0} statistics", self.port_id)));
        complete.with(Style::modern());
        println!("{complete}");
    }

    /// Prints fraction of packets received in software.
    /// If no hardware filters are configured, then a value less than one implies
    /// that incoming traffic is arriving too fast for the CPU to handle.
    /// If there are hardware filters configured, then this value indicates that
    /// fraction of total traffic that was filtered by hardware and successfully
    /// delivered to the processing cores.
    pub(super) fn display_capture_rate(&self) -> Table {
        let captured = self.stats.get("rx_good_packets");
        let total = self.stats.get("rx_phy_packets");

        match (captured, total) {
            (Some(captured), Some(total)) => {
                let capture_rate = *captured as f64 / *total as f64;
                vec![["SW Capture %".into(), format!("{capture_rate}%")]].table()
            }
            _ => vec![["SW Capture %", "UNKOWN"]].table(),
        }
    }

    /// Prints fraction of packets discarded by the NIC due to lack of software buffers
    /// available for the incoming packets, aggregated over all RX queues. A non-zero
    /// value implies that the CPU is not consuming packets fast enough. If there are
    /// no hardware filters configured, this value should be 1 - SW Capture %.
    pub(super) fn display_out_of_buffer_rate(&self) -> Table {
        let discards = self.stats.get("rx_out_of_buffer");
        let total = self.stats.get("rx_phy_packets");

        match (discards, total) {
            (Some(discards), Some(total)) => {
                let discard_rate = *discards as f64 / *total as f64;
                vec![["Out of Buffer %".into(), format!("{discard_rate}%")]].table()
            }
            _ => vec![["Out of Buffer %", "UNKOWN"]].table(),
        }
    }

    /// Prints fraction of packets discarded by the NIC due to lack of buffers on
    /// the physical port. A non-zero value implies that the NIC or bus is congested and
    /// cannot absorb the traffic coming from the network. A value of zero may still
    /// indicate that the CPU is not consuming packets fast enough.
    pub(super) fn display_discard_rate(&self) -> Table {
        let discards = self.stats.get("rx_phy_discard_packets");
        let total = self.stats.get("rx_phy_packets");

        match (discards, total) {
            (Some(discards), Some(total)) => {
                let discard_rate = *discards as f64 / *total as f64;
                vec![["HW Discard %".into(), format!("{discard_rate}%")]].table()
            }
            _ => vec![["HW Discard %", "UNKOWN"]].table(),
        }
    }
}
