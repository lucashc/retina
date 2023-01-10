//! Retina runtime.
//!
//! The runtime initializes the DPDK environment abstraction layer, creates memory pools, launches
//! the packet processing cores, and manages logging and display output.

mod online;
use self::online::*;

use crate::config::*;
use crate::dpdk;
use crate::filter::FilterCtx;
use crate::lcore::SocketId;
use crate::memory::mempool::Mempool;
use crate::subscription::*;

use std::collections::BTreeMap;
use std::ffi::CString;
use std::sync::{Arc, RwLock};

use anyhow::{bail, Result};
use regex::bytes::RegexSet;

/// The Retina runtime.
///
/// The runtime initializes the DPDK environment abstraction layer, creates memory pools, launches
/// the packet processing cores, and manages logging and display output.
pub struct Runtime<'a, S>
where
    S: Subscribable,
{
    #[allow(dead_code)]
    mempools: BTreeMap<SocketId, Mempool>,
    online: OnlineRuntime<'a, S>,
    #[cfg(feature = "timing")]
    subscription: Arc<Subscription<'a, S>>,
}

impl<'a, S> Runtime<'a, S>
where
    S: Subscribable,
{
    /// Creates a new runtime from the `config` settings, filter, and callback.
    ///
    /// # Remarks
    ///
    /// The `factory` parameter is a macro-generated function pointer based on the user-defined
    /// filter string, and must take the value "`filter`". `cb` is the name of the user-defined
    /// callback function.
    ///
    /// # Example
    ///
    /// ```
    /// let mut runtime = Runtime::new(config, filter, callback)?;
    /// ```
    pub fn new(
        config: RuntimeConfig,
        cb: impl Fn(S, &FilterCtx) + 'a,
        filter_ctx: &FilterCtx,
        exit_callback: Arc<impl Fn() + Send + Sync + 'static>
    ) -> Result<Self> {
        let subscription = Arc::new(Subscription::new(cb));

        println!("Initializing Retina runtime...");
        log::info!("Initializing EAL...");
        dpdk::load_drivers();
        {
            let eal_params = config.get_eal_params();
            let eal_params_len = eal_params.len() as i32;

            let mut args = vec![];
            let mut ptrs = vec![];
            for arg in eal_params.into_iter() {
                let s = CString::new(arg).unwrap();
                ptrs.push(s.as_ptr() as *mut u8);
                args.push(s);
            }

            let ret = unsafe { dpdk::rte_eal_init(eal_params_len, ptrs.as_ptr() as *mut _) };
            if ret < 0 {
                bail!("Failure initializing EAL");
            }
        }

        log::info!("Initializing Mempools...");
        let mut mempools = BTreeMap::new();
        let socket_ids = config.get_all_socket_ids();
        let mtu = if let Some(online) = &config.online {
            online.mtu
        } else {
            Mempool::default_mtu()
        };
        for socket_id in socket_ids {
            log::debug!("Socket ID: {}", socket_id);
            let mempool = Mempool::new(&config.mempool, socket_id, mtu)?;
            mempools.insert(socket_id, mempool);
        }

        let online = config.online.as_ref().map(|cfg| {
            log::info!("Initializing Online Runtime...");
            let online_opts = OnlineOptions {
                online: cfg.clone()
            };
            OnlineRuntime::new(
                &config,
                online_opts,
                &mut mempools,
                Arc::clone(&subscription),
                filter_ctx,
                exit_callback
            )
        }).unwrap();

        log::info!("Runtime ready.");
        Ok(Runtime {
            mempools,
            online,
            #[cfg(feature = "timing")]
            subscription,
        })
    }

    /// Run Retina for the duration specified in the configuration or until `ctrl-c` to terminate.
    ///
    /// # Example
    ///
    /// ```
    /// runtime.run();
    /// ```
    pub fn run(&mut self) {
        self.online.run();
        #[cfg(feature = "timing")]
        {
            self.subscription.timers.display_stats();
            self.subscription.timers.dump_stats();
        }
        log::info!("Done.");
    }

    pub fn get_regexes_from_cores(&self) -> Vec<Arc<RwLock<RegexSet>>> {
        self.online.rx_cores.values().map(|core| core.filter_ctx.regexes.clone()).collect()
    }
}
