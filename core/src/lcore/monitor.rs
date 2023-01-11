use crate::config::RuntimeConfig;
use crate::dpdk;
use crate::port::{statistics::PortStats, Port, PortId, RxQueue, RxQueueType};

use std::collections::{BTreeMap, HashMap};
use std::ffi::CString;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use chrono::Local;
use crossbeam_channel::{tick, Receiver};
use csv::Writer;
use serde::Serialize;
use tabled::{builder::Builder, Style};
use tabled::{col, row, Panel, Table};

/// Preamble + Start Frame Delimiter
const PSFD_SIZE: u64 = 8;
/// Interpacket Gap
const IPG_SIZE: u64 = 12;
/// Frame Checksum
const FCS_SIZE: u64 = 4;

/// A Monitor monitors throughput when running online, displays live statistics
#[derive(Debug)]
pub(crate) struct Monitor {
    duration: Option<Duration>,
    display: Option<Display>,
    logger: Option<Logger>,
    ports: BTreeMap<PortId, Vec<RxQueue>>,
    is_running: Arc<AtomicBool>,
}

impl Monitor {
    pub(crate) fn new(
        config: &RuntimeConfig,
        ports: &BTreeMap<PortId, Port>,
        is_running: Arc<AtomicBool>,
    ) -> Self {
        let date = Local::now();
        let online_cfg = config
            .online
            .as_ref()
            .expect("Not configured for online runtime");

        let duration = online_cfg.duration.map(Duration::from_secs);

        let display = (|| {
            if let Some(monitor_cfg) = &online_cfg.monitor {
                if let Some(display_cfg) = &monitor_cfg.display {
                    return Some(Display {
                        ticker: tick(Duration::from_millis(1000)),
                        display_stats: display_cfg.display_stats,
                        keywords: display_cfg.port_stats.clone(),
                    });
                }
            }
            None
        })();

        let logger = (|| {
            if let Some(monitor_cfg) = &online_cfg.monitor {
                if let Some(log_cfg) = &monitor_cfg.log {
                    let path = Path::new(&log_cfg.directory)
                        .join(date.format("%Y-%m-%dT%H:%M:%S").to_string());
                    fs::create_dir_all(&path).expect("create log directory");
                    log::info!("Logging to {:?}", path);

                    let toml = toml::to_string(&config).expect("serialize config");
                    let mut config_file =
                        fs::File::create(path.join("config.toml")).expect("create config log");
                    config_file.write_all(toml.as_bytes()).expect("log config");

                    let mut port_wtrs = HashMap::new();
                    for port_id in ports.keys() {
                        let fname = path.join(format!("port{}.csv", port_id));
                        let wtr = Writer::from_path(&fname).expect("create portstat log");
                        port_wtrs.insert(*port_id, wtr);
                    }
                    return Some(Logger {
                        ticker: tick(Duration::from_millis(log_cfg.interval)),
                        path,
                        port_wtrs,
                        keywords: log_cfg.port_stats.clone(),
                    });
                }
            }
            None
        })();

        let mut monitor_ports: BTreeMap<PortId, Vec<RxQueue>> = BTreeMap::new();
        for (port_id, port) in ports.iter() {
            monitor_ports.insert(*port_id, port.queue_map.keys().cloned().collect());
        }

        Monitor {
            duration,
            display,
            logger,
            ports: monitor_ports,
            is_running,
        }
    }

    pub(crate) fn run(&mut self) {
        if let Some(logger) = &mut self.logger {
            logger.init_port_wtrs().expect("port logger init");
        }
        // ts of run start
        let start_ts = Instant::now();
        // initial data capture
        let mut init_rx = AggRxStats::default();
        // ts of initial data capture
        let mut init_ts = start_ts;

        let mut prev_rx = init_rx;
        let mut prev_ts = init_ts;
        let mut init = true;
        // Add a small delay to allow workers to start polling for packets
        std::thread::sleep(Duration::from_millis(1000));
        while self.is_running.load(Ordering::Relaxed) {
            if let Some(duration) = self.duration {
                if start_ts.elapsed() >= duration {
                    self.is_running.store(false, Ordering::Relaxed);
                }
            }

            if let Some(display) = &self.display {
                if display.ticker.try_recv().is_ok() {
                    let curr_ts = Instant::now();
                    let delta = curr_ts - prev_ts;
                    match AggRxStats::collect(&self.ports, &display.keywords) {
                        Ok(curr_rx) => {
                            let nms = delta.as_millis() as f64;
                            if init {
                                init_rx = curr_rx;
                                init_ts = curr_ts;
                                init = false;
                            }
                            if display.display_stats {
                                let mempool_table = display.mempool_usage(&self.ports);
                                let rates_table = AggRxStats::display_rates(curr_rx, prev_rx, nms);
                                let dropped_table = AggRxStats::display_dropped(curr_rx, init_rx);
                                let mut tmp_row = row![rates_table, dropped_table];
                                tmp_row.with(Style::modern());
                                let mut overall = col![mempool_table, tmp_row];
                                overall.with(Panel::header(format!(
                                    "Overall statistics\nCurrent time: {}s",
                                    (curr_ts - start_ts).as_secs()
                                )));
                                overall.with(Style::modern());
                                println!("{overall}");
                            }
                            prev_rx = curr_rx;
                            prev_ts = curr_ts;
                        }
                        Err(error) => {
                            log::error!("Monitor display error: {}", error);
                        }
                    }
                }
            }

            if let Some(logger) = &mut self.logger {
                if logger.ticker.try_recv().is_ok() {
                    match logger.log_stats(init_ts.elapsed()) {
                        Ok(_) => (),
                        Err(error) => log::error!("Monitor log error: {}", error),
                    }
                }
            }
        }

        std::thread::sleep(Duration::from_millis(100));
        println!("----------------------------------------------");
        let tputs = Throughputs::new(prev_rx, init_rx, (prev_ts - init_ts).as_millis() as f64);
        println!("{}", tputs);

        if let Some(logger) = &self.logger {
            let json_fname = logger.path.join("throughputs.json");
            tputs.dump_json(json_fname).expect("Unable to dump to json");
        }
    }
}

#[derive(Debug)]
struct Display {
    ticker: Receiver<Instant>,
    display_stats: bool,
    keywords: Vec<String>,
}

impl Display {
    /// Display mempool usage
    fn mempool_usage(&self, ports: &BTreeMap<PortId, Vec<RxQueue>>) -> Table {
        let mut total = Builder::default();
        for name in ports.keys().map(|id| format!("mempool_{}", id.socket_id())) {
            let cname = CString::new(name.clone()).expect("Invalid CString conversion");
            let mempool_raw = unsafe { dpdk::rte_mempool_lookup(cname.as_ptr()) };
            let avail_cnt = unsafe { dpdk::rte_mempool_avail_count(mempool_raw) };
            let inuse_cnt = unsafe { dpdk::rte_mempool_in_use_count(mempool_raw) };

            let mut builder = Builder::default();
            builder.add_record(["Available".into(), format!("{avail_cnt} MBufs")]);
            builder.add_record(["Usage".into(), format!("{inuse_cnt} MBufs")]);
            let usage = 100.0 * inuse_cnt as f64 / (inuse_cnt + avail_cnt) as f64;
            builder.add_record(["Percentage".into(), format!("{usage}%")]);

            let mut table = builder.build();
            table.with(Panel::header(format!("Mempool {name} statistics")));
            table.with(Style::modern());
            total.add_record([table.to_string()]);
        }
        let mut total_table = total.build();
        total_table.with(Panel::header("Mempools"));
        total_table.with(Style::modern());
        return total_table;
    }
}

#[derive(Debug)]
struct Logger {
    ticker: Receiver<Instant>,
    path: PathBuf,
    port_wtrs: HashMap<PortId, Writer<std::fs::File>>,
    keywords: Vec<String>,
}

impl Logger {
    /// Initialize port statistic CSV writers. Must occur after ports have been started.
    fn init_port_wtrs(&mut self) -> Result<()> {
        for (port_id, wtr) in self.port_wtrs.iter_mut() {
            let port_stats = PortStats::collect(*port_id)?;
            wtr.write_field("ts")?;
            for label in port_stats.stats.keys() {
                if self.keywords.iter().any(|k| label.contains(k)) {
                    wtr.write_field(label)?;
                }
            }
            wtr.write_field("mempool_avail_cnt")?;
            wtr.write_field("mempool_inuse_cnt")?;
            wtr.write_record(None::<&[u8]>)?;
            wtr.flush()?;
        }
        Ok(())
    }

    /// Logs per-port statistics and mempool statistics (per-socket statistics).
    fn log_stats(&mut self, elapsed: Duration) -> Result<()> {
        for (port_id, wtr) in self.port_wtrs.iter_mut() {
            let port_stats = PortStats::collect(*port_id);
            match port_stats {
                Ok(port_stats) => {
                    wtr.write_field(elapsed.as_millis().to_string())?;
                    for label in port_stats.stats.keys() {
                        if self.keywords.iter().any(|k| label.contains(k)) {
                            if let Some(value) = port_stats.stats.get(label) {
                                wtr.write_field(value.to_string())?;
                            } else {
                                wtr.write_field("-")?;
                            }
                        }
                    }
                }
                Err(error) => log::error!("{}", error),
            }
            let name = format!("mempool_{}", port_id.socket_id());
            let cname = CString::new(name.clone()).expect("Invalid CString conversion");
            let mempool_raw = unsafe { dpdk::rte_mempool_lookup(cname.as_ptr()) };
            let avail_cnt = unsafe { dpdk::rte_mempool_avail_count(mempool_raw) };
            let inuse_cnt = unsafe { dpdk::rte_mempool_in_use_count(mempool_raw) };
            wtr.write_field(avail_cnt.to_string())?;
            wtr.write_field(inuse_cnt.to_string())?;
            wtr.write_record(None::<&[u8]>)?;
        }
        for wtr in self.port_wtrs.values_mut() {
            wtr.flush()?;
        }
        Ok(())
    }
}

/// Aggregate RX port statistics at time of collection
#[derive(Debug, Default, Clone, Copy)]
struct AggRxStats {
    ingress_bits: u64,
    ingress_pkts: u64,
    good_bits: u64,
    good_pkts: u64,
    process_bits: u64,
    process_pkts: u64,
    hw_dropped_pkts: u64,
    sw_dropped_pkts: u64,
}

impl AggRxStats {
    /// Collect aggregate statistics, display keyword statistics if `keywords` is not `None`
    fn collect(ports: &BTreeMap<PortId, Vec<RxQueue>>, keywords: &[String]) -> Result<Self> {
        let mut ingress_bytes = 0;
        let mut ingress_pkts = 0;
        let mut good_bytes = 0;
        let mut good_pkts = 0;
        let mut process_bytes = 0;
        let mut process_pkts = 0;
        let mut hw_dropped_pkts = 0;
        let mut sw_dropped_pkts = 0;
        for (port_id, rx_queues) in ports.iter() {
            let mut sink_queue = None;
            for queue in rx_queues {
                if queue.ty == RxQueueType::Sink {
                    sink_queue = Some(queue.qid.raw());
                }
            }

            match PortStats::collect(*port_id) {
                Ok(port_stats) => {
                    // Ingress (reached NIC)
                    ingress_bytes += match port_stats.stats.get("rx_phy_bytes") {
                        Some(v) => *v,
                        None => {
                            log::warn!("Failed retrieving ingress_bytes, device does not support precise PHY count");
                            0
                        }
                    };
                    ingress_pkts += match port_stats.stats.get("rx_phy_packets") {
                        Some(v) => *v,
                        None => {
                            log::warn!("Failed retrieving ingress_pkts, device does not support precise PHY count");
                            0
                        }
                    };

                    // Good (reached software)
                    let good_bytes_temp = match port_stats.stats.get("rx_good_bytes") {
                        Some(v) => *v,
                        None => {
                            log::warn!("Failed retrieving good_bytes, device does not support precise PHY count");
                            0
                        }
                    };
                    let good_pkts_temp = match port_stats.stats.get("rx_good_packets") {
                        Some(v) => *v,
                        None => {
                            log::warn!("Failed retrieving good_pkts, device does not support precise PHY count");
                            0
                        }
                    };
                    good_bytes += good_bytes_temp;
                    good_pkts += good_pkts_temp;

                    // Process (reached workers)
                    process_bytes += if let Some(sink) = sink_queue {
                        let label = format!("rx_q{}_bytes", sink);
                        let sink_bytes = match port_stats.stats.get(&label) {
                            Some(v) => *v,
                            None => bail!("Failed retrieving sink_bytes"),
                        };
                        good_bytes_temp - sink_bytes
                    } else {
                        good_bytes_temp
                    };
                    process_pkts += if let Some(sink) = sink_queue {
                        let label = format!("rx_q{}_packets", sink);
                        let sink_pkts = match port_stats.stats.get(&label) {
                            Some(v) => *v,
                            None => bail!("Failed retrieving sink_pkts"),
                        };
                        good_pkts_temp - sink_pkts
                    } else {
                        good_pkts_temp
                    };

                    // dropped
                    hw_dropped_pkts += match port_stats.stats.get("rx_phy_discard_packets") {
                        Some(v) => *v,
                        None => {
                            log::warn!("Failed retrieving hw_dropped_pkts, device does not support precise packet dropped counter (no hardware drop will be accounted for).");
                            0
                        }
                    };
                    sw_dropped_pkts += match port_stats.stats.get("rx_missed_errors") {
                        Some(v) => *v,
                        None => bail!("Failed retrieving sw_dropped_pkts"),
                    };

                    port_stats.display(keywords);
                }
                Err(error) => bail!(error),
            }
        }
        Ok(AggRxStats {
            ingress_bits: (ingress_bytes + (PSFD_SIZE + IPG_SIZE) * ingress_pkts) * 8,
            ingress_pkts,
            good_bits: (good_bytes + (PSFD_SIZE + IPG_SIZE + FCS_SIZE) * good_pkts) * 8,
            good_pkts,
            process_bits: (process_bytes + (PSFD_SIZE + IPG_SIZE + FCS_SIZE) * process_pkts) * 8,
            process_pkts,
            hw_dropped_pkts,
            sw_dropped_pkts,
        })
    }

    /// Display live bits per second and packets per second between `curr_rx` and `prev_rx`
    fn display_rates(curr_rx: AggRxStats, prev_rx: AggRxStats, nms: f64) -> Table {
        let mut builder = Builder::default();

        builder.add_record([
            "Ingress".into(),
            format!(
                "{} bps / {} pps",
                (curr_rx.ingress_bits - prev_rx.ingress_bits) as f64 / nms * 1000.0,
                (curr_rx.ingress_pkts - prev_rx.ingress_pkts) as f64 / nms * 1000.0
            ),
        ]);
        builder.add_record([
            "Good".into(),
            format!(
                "{} bps / {} pps",
                (curr_rx.good_bits - prev_rx.good_bits) as f64 / nms * 1000.0,
                (curr_rx.good_pkts - prev_rx.good_pkts) as f64 / nms * 1000.0
            ),
        ]);
        builder.add_record([
            "Process".into(),
            format!(
                "{} bps / {} pps",
                (curr_rx.process_bits - prev_rx.process_bits) as f64 / nms * 1000.0,
                (curr_rx.process_pkts - prev_rx.process_pkts) as f64 / nms * 1000.0
            ),
        ]);
        builder.add_record([
            "Drop".into(),
            format!(
                "{} pps ({}%)",
                (curr_rx.dropped_pkts() - prev_rx.dropped_pkts()) as f64 / nms * 1000.0,
                100.0
                    * ((curr_rx.dropped_pkts() - prev_rx.dropped_pkts()) as f64
                        / (curr_rx.ingress_pkts - prev_rx.ingress_pkts) as f64)
            ),
        ]);
        let mut table = builder.build();
        table.with(Panel::header("Current rates"));
        table.with(Style::modern());
        return table;
    }

    fn display_dropped(curr_rx: AggRxStats, init_rx: AggRxStats) -> Table {
        let mut builder = Builder::default();
        builder.add_record([
            "HW Dropped".into(),
            format!(
                "{} pkts ({}%)",
                curr_rx.hw_dropped_pkts - init_rx.hw_dropped_pkts,
                100.0
                    * ((curr_rx.hw_dropped_pkts - init_rx.hw_dropped_pkts) as f64
                        / (curr_rx.ingress_pkts - init_rx.ingress_pkts) as f64)
            ),
        ]);
        builder.add_record([
            "SW Dropped".into(),
            format!(
                "{} pkts ({}%)",
                curr_rx.sw_dropped_pkts - init_rx.sw_dropped_pkts,
                100.0
                    * ((curr_rx.sw_dropped_pkts - init_rx.sw_dropped_pkts) as f64
                        / (curr_rx.ingress_pkts - init_rx.ingress_pkts) as f64)
            ),
        ]);
        builder.add_record([
            "Total Dropped".into(),
            format!(
                "{} pkts ({}%)",
                curr_rx.dropped_pkts() - init_rx.dropped_pkts(),
                100.0
                    * ((curr_rx.dropped_pkts() - init_rx.dropped_pkts()) as f64
                        / (curr_rx.ingress_pkts - init_rx.ingress_pkts) as f64)
            ),
        ]);
        let mut table = builder.build();
        table.with(Panel::header("Overall Drop"));
        table.with(Style::modern());
        table
    }

    fn dropped_pkts(&self) -> u64 {
        self.hw_dropped_pkts + self.sw_dropped_pkts
    }
}

#[derive(Debug, Serialize)]
struct Throughputs {
    avg_ingress_bps: f64,
    avg_ingress_pps: f64,
    avg_good_bps: f64,
    avg_good_pps: f64,
    avg_process_bps: f64,
    avg_process_pps: f64,
    hw_dropped_pkts: u64,
    sw_dropped_pkts: u64,
    tot_dropped_pkts: u64,
    percent_dropped: f64,
}

impl Throughputs {
    /// Compute average rates over elapsed time
    fn new(curr_rx: AggRxStats, init_rx: AggRxStats, ems: f64) -> Self {
        Throughputs {
            avg_ingress_bps: (curr_rx.ingress_bits - init_rx.ingress_bits) as f64 / ems * 1000.0,
            avg_ingress_pps: (curr_rx.ingress_pkts - init_rx.ingress_pkts) as f64 / ems * 1000.0,
            avg_good_bps: (curr_rx.good_bits - init_rx.good_bits) as f64 / ems * 1000.0,
            avg_good_pps: (curr_rx.good_pkts - init_rx.good_pkts) as f64 / ems * 1000.0,
            avg_process_bps: (curr_rx.process_bits - init_rx.process_bits) as f64 / ems * 1000.0,
            avg_process_pps: (curr_rx.process_pkts - init_rx.process_pkts) as f64 / ems * 1000.0,
            hw_dropped_pkts: (curr_rx.hw_dropped_pkts - init_rx.hw_dropped_pkts),
            sw_dropped_pkts: (curr_rx.sw_dropped_pkts - init_rx.sw_dropped_pkts),
            tot_dropped_pkts: (curr_rx.dropped_pkts() - init_rx.dropped_pkts()),
            percent_dropped: 100.0
                * ((curr_rx.dropped_pkts() - init_rx.dropped_pkts()) as f64
                    / (curr_rx.ingress_pkts - init_rx.ingress_pkts) as f64),
        }
    }

    fn dump_json(&self, path: PathBuf) -> Result<()> {
        let file = std::fs::File::create(path)?;
        serde_json::to_writer(&file, self)?;
        Ok(())
    }
}

impl fmt::Display for Throughputs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "AVERAGE Ingress: {:.3} bps / {:.3} pps",
            self.avg_ingress_bps, self.avg_ingress_pps,
        )?;
        writeln!(
            f,
            "AVERAGE Good:    {:.3} bps / {:.3} pps",
            self.avg_good_bps, self.avg_good_pps,
        )?;
        writeln!(
            f,
            "AVERAGE Process: {:.3} bps / {:.3} pps",
            self.avg_process_bps, self.avg_process_pps,
        )?;
        writeln!(
            f,
            "DROPPED: {} pkts ({}%)",
            self.tot_dropped_pkts, self.percent_dropped,
        )?;
        Ok(())
    }
}
